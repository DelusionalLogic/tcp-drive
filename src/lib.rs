
// `error_chain!` can recurse deeply
#![recursion_limit = "1024"]

// Import the macro. Don't forget to add `error-chain` in your
// `Cargo.toml`!
#[macro_use]
extern crate error_chain;

#[macro_use]
extern crate log;
extern crate clap;
extern crate byteorder;
extern crate ansi_term;
extern crate pbr;

mod network;

use std::path::PathBuf;
use std::io::{Read, Write};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use ansi_term::Colour::*;
use std::cmp::Ordering;
use pbr::{ProgressBar, Units};

pub mod errors {
    use std::error;
    use network;
    use std::io;
    use std::fmt;
    use std::net;
    use std::path;
    error_chain! {
        // The type defined for this error. These are the conventional
        // and recommended names, but they can be arbitrarily chosen.
        //
        // It is also possible to leave this section out entirely, or
        // leave it empty, and these names will be used automatically.
        types {
            Error, ErrorKind, ResultExt, Result;
        }

        // Without the `Result` wrapper:
        //
        // types {
        //     Error, ErrorKind, ResultExt;
        // }

        // Automatic conversions between this error chain and other
        // error chains. In this case, it will e.g. generate an
        // `ErrorKind` variant called `Another` which in turn contains
        // the `other_error::ErrorKind`, with conversions from
        // `other_error::Error`.
        //
        // Optionally, some attributes can be added to a variant.
        //
        // This section can be empty.
        links {
        }

        // Automatic conversions between this error chain and other
        // error types not defined by the `error_chain!`. These will be
        // wrapped in a new error with, in the first case, the
        // `ErrorKind::Fmt` variant. The description and cause will
        // forward to the description and cause of the original error.
        //
        // Optionally, some attributes can be added to a variant.
        //
        // This section can be empty.
        foreign_links {
            Io(io::Error) #[cfg(unix)];
        }

        // Define additional `ErrorKind` variants. The syntax here is
        // the same as `quick_error!`, but the `from()` and `cause()`
        // syntax is not supported.
        errors {
            PathConversion {
                description("Failed coverting the path to a string")
                display("Failed converting path to string")
            }
            Serialization {
                description("Serialization failed")
                display("Failed serializing")
            }
            ServerConnection {
                description("While processing connection")
                display("A low level error occured while processing connection")
            }
            ClientConnection(ip: net::Ipv4Addr, port: u16) {
                description("Client failed to connect to server")
                display("While connecting to {}:{}", ip, port)
            }
            Enumeration {
                description("While enumerating interface")
                display("While enumerating interfaces")
            }
            Bind(ip: &'static str, port: u16) {
                description("While binding connection")
                display("While binding to {}:{}", ip, port)
            }
            IncompleteRead(actual: usize, expected: usize) {
                description("An error occured which caused a read to end before getting the expected data")
                display("A read didn't get the expected amount of data [Expected {}, Actual {}]", actual, expected)
            }
            Fetch {
                description("While reading message")
                display("While reading message")
            }
            InvalidTransport(t: String) {
                description("Transport not valid")
                display("Invalid transport: {}", t)
            }
            FileExists(p: ::std::path::PathBuf) {
                description("File already exists")
                display("Tried to write to existing file: {}", p.to_string_lossy())
            }
            WriteContent {
                description("An error occured while writing content to disk")
                display("While writing content to disk")
            }
            ReadContent {
                description("An error occured while reading content from network")
                display("While reading content from network")
            }
        }
    }
}
use errors::*;

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
    fn read<T: Read + 'a>(stream: T) -> Result<Self> where Self: std::marker::Sized;
    fn write<T: Write + 'a>(&mut self, stream: &mut T) -> Result<usize>;
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
    fn read<T: Read + 'a>(mut stream: T) -> Result<Self> {
        //Get the length of the name
        let name_len = try!(stream.read_u32::<BigEndian>());

        //Get the name from the stream
        let mut name_buff = Vec::with_capacity(name_len as usize); //@Expansion: Here we have the 32-bit again.
        let name_read = try!(stream.readn(&mut name_buff, name_len as usize));
        if name_len != name_read as u32 {
            bail!(ErrorKind::IncompleteRead(name_read, name_len as usize));
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

    fn write<T: Write + 'a>(&mut self, mut stream: &mut T) -> Result<usize>{
        try!(stream.write_u32::<BigEndian>(self.name_size)); //@Error: Should this be handled differently?
        try!(stream.write_all(self.name.as_bytes()));
        try!(stream.write_u32::<BigEndian>(self.size));
        try!(std::io::copy(&mut self.file, &mut stream));
        return Ok(0);
    }
}

pub type Dict<'a> = Box<[&'a str]>;

pub struct TransportPresenter<'a> {
    dictionary: Dict<'a>,
    dict_entries: u32,
}

impl<'a> TransportPresenter<'a> {
    pub fn new(dictionary: Dict<'a>, dict_entries: u32) -> Self {
        return TransportPresenter {
            dictionary: dictionary,
            dict_entries: dict_entries,
        };
    }

    pub fn present(&self, t: &Transportable) -> Result<String> {
        let transport = t.make_transport_context()
            .chain_err(|| "While making transport")?;
        print!("State: {}, max_state: {}, dict_entries: {}\n", transport.state, transport.max_state, self.dict_entries);
        let parts = (transport.max_state as f64).log(self.dict_entries as f64).ceil() as u32;

        let mut part_representation: Vec<&str> = Vec::with_capacity(parts as usize);

        let mut remainder = transport.state;
        for _ in 0..parts {
            let part = remainder % self.dict_entries;
            remainder = remainder / self.dict_entries;
            print!("Part: {}\n", part);
            part_representation.push(self.dictionary[part as usize]);
        }
        return Ok(part_representation.join(" "));
    }

    pub fn present_inv<T: Transportable>(&self, s: String) -> Result<T> {
        let mut res:  u32 = 0;
        let mut part_count = 0;
        for word in s.split(" ") {
            if let Ok(val) = self.dictionary.binary_search_by(|p| {
                //Flip the search to allow for cmp between String and &str
                match word.cmp(p) {
                    Ordering::Greater => Ordering::Less,
                    Ordering::Less => Ordering::Greater,
                    Ordering::Equal => Ordering::Equal,
                }
            }) {
                res += (val as u32) * (self.dict_entries.pow(part_count));
                part_count += 1;
            } else {
                bail!(ErrorKind::InvalidTransport(word.to_owned()));
            }
        }
        return T::from_transport_s(res);
    }
}

pub struct Transport {
    state: u32,
    max_state: u32,
}

pub trait Transportable {
    fn make_transport_context(&self) -> Result<Transport>;
    fn from_transport_s(t: u32) -> Result<Self> where Self: std::marker::Sized;
    fn make_transport(&self, dict: &Dict) -> String;
    fn from_transport<'a, S: Into<&'a String>>(dict: &Dict, transport: S) -> Result<Self>
        where Self: std::marker::Sized;
}

impl Transportable for std::net::Ipv4Addr {
    fn make_transport_context(&self) -> Result<Transport> {
        return Ok(Transport {
            state: u32::from(self.clone()),
            max_state: std::u32::MAX,
        })
    }
    fn from_transport_s(t: u32) -> Result<Self> {
        return Ok(std::net::Ipv4Addr::from(t));
    }
    fn make_transport(&self, dict: &Dict) -> String {
        let transport = self.octets()
            .chunks(2)
            .map(| item | item[1] as u16 | (item[0] as u16) << 8) //@Expansion: We only support 2 bytes per word here
            .map(| i | dict[i as usize].to_owned()) //@Hack: Can we do this without owned?
            .collect::<Vec<_>>()
            .join(" ");
        return transport
    }

    fn from_transport<'a, S: Into<&'a String>>(dict: &Dict, transport: S) -> Result<Self> {
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
                bail!(ErrorKind::InvalidTransport(word.to_owned()));
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

    pub fn from_path(path: PathBuf) -> Result<FileInfo> {
        let metadata = std::fs::metadata(&path)?;
        return Ok(FileInfo::new(path, metadata.len()))
    }

    pub fn open(&self) -> std::result::Result<std::fs::File, std::io::Error> {
        return std::fs::File::open(&self.path);
    }
}

fn send_file<S: Write>(mut stream: &mut S, file: &FileInfo) -> Result<()> {
    let filename = match file.path.file_name()
        .and_then(|x| x.to_str())
        .map(|x| x.to_owned()) {
        Some(x) => x,
        None => return Err(ErrorKind::PathConversion.into()),
    };

    let mut message = FileMessage::new(filename, file.len as u32, try!(file.open()));
    message.write(&mut stream)
        .chain_err(|| ErrorKind::Serialization)?;
    return Ok(());
}

pub fn serve_file(file: FileInfo, port: u16) -> Result<()> {
        let listener = std::net::TcpListener::bind(("0.0.0.0", port))
            .chain_err(|| ErrorKind::Bind("0.0.0.0", port))?;

        for conn in listener.incoming() {
            let mut stream = conn.chain_err(|| ErrorKind::ServerConnection)?;

            send_file(&mut stream, &file);
        }
        return Ok(());
}

pub fn fetch_file(presenter: TransportPresenter, key: String, file: Option<std::path::PathBuf>) ->  Result<()> {
    let ip: std::net::Ipv4Addr = presenter.present_inv(key).unwrap();
    println!("{} from ip {}",
             Green.paint("Downloading"),
             Yellow.paint(ip.to_string()));
    //@Expansion: We can't time out right now. Use the net2::TcpBuilder?
    let stream = std::net::TcpStream::connect((ip, 2222))
        .chain_err(|| ErrorKind::ClientConnection(ip, 2222))?;
    let mut message = FileMessage::read(stream)
        .chain_err(|| ErrorKind::Fetch)?;


    let mut pb = ProgressBar::new(message.size as u64);
    pb.set_units(Units::Bytes);

    let new_path = file
        .unwrap_or(std::path::PathBuf::from(&message.name));

    if new_path.exists() {
        bail!(ErrorKind::FileExists(new_path));
    }

    let mut file = try!(std::fs::File::create(new_path));

    let mut buffer = [0u8; 8192];
    loop{
        let read = message.file.read(&mut buffer)
            .chain_err(|| ErrorKind::ReadContent)?;
        if read == 0 {
            break;
        }
        pb.add(read as u64);
        file.write(&mut buffer[0..read])
            .chain_err(|| ErrorKind::WriteContent)?;
    }
    return Ok(());
}

