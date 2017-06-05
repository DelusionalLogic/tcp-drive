#[macro_use]
extern crate log;
extern crate clap;
extern crate byteorder;
extern crate ansi_term;
extern crate pbr;

mod network;
mod errors;

use std::path::PathBuf;
use errors::*;
use std::io::{Read, Write};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use ansi_term::Colour::*;
use std::cmp::Ordering;
use pbr::{ProgressBar, Units};

#[macro_export]
macro_rules! atry {
    ($expr:expr, $map:expr) => (
        try!($expr.map_err($map))
    );
}

trait Readn {
    fn readn(&mut self, buff: &mut Vec<u8>, n: usize) -> std::io::Result<usize>;
}

impl<T: Read> Readn for T {
    fn readn(&mut self, mut buff: &mut Vec<u8>, n: usize) -> std::io::Result<usize> {
        let mut sub = self.take(n as u64);
        return sub.read_to_end(&mut buff);
    }
}

trait Streamable<'a>{
    fn read<T: Read + 'a>(stream: T) -> Result<Self, SerializationError> where Self: std::marker::Sized;
    fn write<T: Write + 'a>(&mut self, stream: &mut T) -> Result<usize, SerializationError>;
}

struct FileMessage<'a> {
    name_size: u32,
    name: String,
    size: u32,
    file: Box<Read + 'a>,
}

impl<'a> FileMessage<'a> {
    fn new<T: Read + 'a>(name: String, size: u32, stream: T) -> Self {
        return FileMessage {
            name_size:  name.len() as u32, //@Expansion: 32 bits is a lot, but maybe in the far flung future.
            name: name,
            size: size,
            file: Box::new(stream)
        };
    }
}

impl<'a> Streamable<'a> for FileMessage<'a> {
    fn read<T: Read + 'a>(mut stream: T) -> Result<Self, SerializationError> {
        //Get the length of the name
        let name_len = try!(stream.read_u32::<BigEndian>());

        //Get the name from the stream
        let mut name_buff = Vec::with_capacity(name_len as usize); //@Expansion: Here we have the 32-bit again.
        let name_read = try!(stream.readn(&mut name_buff, name_len as usize));
        if name_len != name_read as u32 {
            return Err(SerializationError::IncompleteRead(name_read, name_len as usize));
        }
        let name = String::from_utf8(name_buff).unwrap(); //@Error: Make error

        //Get the length of the file contents
        let file_len = try!(stream.read_u32::<BigEndian>()); //@Expansion: u32. That's a direct limit on the size of files.
                                                             //Currently we aren't aiming at
                                                             //supporting large files, which makes
                                                             //it ok.
        //We aren't getting the file contents because we don't want to store it all in memory
        return Ok(FileMessage {
            name_size: name_len,
            name: name,
            size: file_len,
            file: Box::new(stream),
        });
    }

    fn write<T: Write + 'a>(&mut self, mut stream: &mut T) -> Result<usize, SerializationError>{
        try!(stream.write_u32::<BigEndian>(self.name_size)); //@Error: Should this be handled differently?
        try!(stream.write_all(self.name.as_bytes()));
        try!(stream.write_u32::<BigEndian>(self.size));
        try!(std::io::copy(&mut self.file, &mut stream));
        return Ok(0);
    }
}

pub type Dict<'a> = Box<[&'a str]>;

pub trait Transportable {
    fn make_transport(&self, dict: &Dict) -> String;
    fn from_transport<'a, S: Into<&'a String>>(dict: &Dict, transport: S) -> Result<Self, FetchError>
        where Self: std::marker::Sized;
}

impl Transportable for std::net::Ipv4Addr {
    fn make_transport(&self, dict: &Dict) -> String {
        let transport = self.octets()
            .chunks(2)
            .map(| item | item[1] as u16 | (item[0] as u16) << 8) //@Expansion: We only support 2 bytes per word here
            .map(| i | dict[i as usize].to_owned()) //@Hack: Can we do this without owned?
            .collect::<Vec<_>>()
            .join(" ");
        return transport
    }

    fn from_transport<'a, S: Into<&'a String>>(dict: &Dict, transport: S) -> Result<Self, FetchError> {
        let transport : &String = transport.into();
        let mut ip_vec = Vec::new();

        for word in transport.split(" ") {
            if let Ok(val) = dict.binary_search_by(|p| {
                    //Flip the search to allow for cmp between String and &str
                    match word.cmp(p) {
                        Ordering::Greater => Ordering::Less,
                        Ordering::Less => Ordering::Greater,
                        Ordering::Equal => Ordering::Equal,
                    }
                }) {
                ip_vec.push(val as u32);
            } else {
                return Err(FetchError::InvalidTransport(word.to_owned()));
            }
        }

        return Ok(std::net::Ipv4Addr::from(ip_vec[1] |  ip_vec[0] << 16));
    }
}

pub struct FileInfo{
    path: PathBuf,
    len: u64,
}

impl FileInfo {
    fn new(path: PathBuf, len: u64) -> FileInfo {
        return FileInfo {
            path: path,
            len: len,
        }
    }

    pub fn from_path(path: PathBuf) -> Result<FileInfo, ServeError> {
        let metadata = try!(std::fs::metadata(&path));
        return Ok(FileInfo::new(path, metadata.len()))
    }

    pub fn open(&self) -> Result<std::fs::File, std::io::Error> {
        return Ok(try!(std::fs::File::open(&self.path)));
    }
}

//@Refactor for @Test: Dont open the file here

fn send_file<S: Write>(mut stream: &mut S, file: &FileInfo) -> Result<(), SendError> {
    let filename = match file.path.file_name()
        .and_then(|x| x.to_str())
        .map(|x| x.to_owned()) {
        Some(x) => x,
        None => return Err(SendError::PathConversion),
    };

    let mut message = FileMessage::new(filename, file.len as u32, try!(file.open()));
    atry!(message.write(&mut stream), SendError::Serialization);
    return Ok(());
}

pub fn serve_file(lines: &Dict, file: FileInfo, port: u16) -> Result<(), ServeError> {
        let listener = atry!(std::net::TcpListener::bind(("0.0.0.0", port)), | err | ServeError::Bind(err, "0.0.0.0", port));

        for conn in listener.incoming() {
            let mut stream = atry!(conn, ServeError::Connection);

            atry!(send_file(&mut stream, &file), ServeError::SendingFile);
        }
        return Ok(());
}


pub fn fetch_file(lines: &Dict, key: String, file: Option<std::path::PathBuf>) ->  Result<(), FetchError> {
        let ip = try!(std::net::Ipv4Addr::from_transport(lines, &key));
        println!("{} from ip {}",
                 Green.paint("Downloading"),
                 Yellow.paint(ip.to_string()));
        //@Expansion: We can't time out right now. Use the net2::TcpBuilder?
        let stream = atry!(std::net::TcpStream::connect((ip, 2222)), | err | FetchError::Connection(err, ip, 2222));
        let mut message = atry!(FileMessage::read(stream), FetchError::ReadMessage);


        let mut pb = ProgressBar::new(message.size as u64);
        pb.set_units(Units::Bytes);

        let new_path = file
            .unwrap_or(std::path::PathBuf::from(&message.name));

        if new_path.exists() {
            return Err(FetchError::FileExists(new_path));
        }

        let mut file = try!(std::fs::File::create(new_path));

        let mut buffer = [0u8; 8192];
        loop{
            let read = atry!(message.file.read(&mut buffer), FetchError::ReadContent);
            if read == 0 {
                break;
            }
            pb.add(read as u64);
            atry!(file.write(&mut buffer[0..read]), FetchError::WriteContent);
        }
    return Ok(());
}

