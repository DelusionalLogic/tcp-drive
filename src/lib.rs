
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

pub mod network;

use std::path::PathBuf;
use std::io::{Read, Write};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use ansi_term::Colour::*;
use std::cmp::Ordering;
use pbr::{ProgressBar, Units};

pub mod errors {
    use std::io;
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
            //I think cloning the pathbuf is ok for the slow path in case of error
            SendFile(remote_addr: net::SocketAddr){
                description("Error while sending file")
                display("While sending to {}", remote_addr)
            }
            UnknownFile(index: u32) {
                description("The client requested an unknown file")
                display("The client requested an unknown file with id {}", index)
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
            Bind(ip: net::Ipv4Addr, port: u16) {
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

    pub fn present(&self, t: &Transport) -> Result<String> {
        let parts = (t.max_state() as f64).log(self.dict_entries as f64).ceil() as u32;

        let mut part_representation: Vec<&str> = Vec::with_capacity(parts as usize);

        let mut remainder = t.state();
        for _ in 0..parts {
            let part = remainder % self.dict_entries;
            remainder = remainder / self.dict_entries;
            part_representation.push(self.dictionary[part as usize]);
        }
        return Ok(part_representation.join(" "));
    }

    pub fn present_inv(&self, s: String) -> Result<ClientTransport> {
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
        return Ok(ClientTransport::new(res));
    }
}

pub struct ServerTransport {
    state: u32,
    max_state: u32,
}

pub struct ClientTransport {
    state: u32,
}

pub trait Transport {
    fn state(&self) -> u32;
    fn max_state(&self) -> u32;
}

impl ServerTransport {
    fn new(state: u32, max_state: u32) -> Self {
        return ServerTransport {
            state: state,
            max_state: max_state,
        };
    }
}

impl Transport for ServerTransport {
    fn state(&self) -> u32 {
        return self.state;
    }

    fn max_state(&self) -> u32 {
        return self.max_state;
    }
}

pub trait PartialTransport {
    fn state(&self) -> u32;
}

impl ClientTransport {
    fn new(state: u32) -> Self {
        return ClientTransport {
            state: state,
        };
    }
}

impl PartialTransport for ClientTransport {
    fn state(&self) -> u32 {
        return self.state;
    }
}

impl <T: Transport> PartialTransport for T {
    fn state(&self) -> u32 {
        return Transport::state(self);
    }
}

pub trait Transportable {
    fn make_transport(&self) -> Result<ServerTransport>;
    fn from_transport<T: PartialTransport>(t: T) -> Result<Self> where Self: std::marker::Sized;
}

impl Transportable for std::net::Ipv4Addr {
    fn make_transport(&self) -> Result<ServerTransport> {
        return Ok(ServerTransport::new(u32::from(self.clone()), std::u32::MAX));
    }

    fn from_transport<T: PartialTransport>(t: T) -> Result<Self> {
        return Ok(std::net::Ipv4Addr::from(t.state()));
    }
}

#[derive(Clone)]
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

//@Refactor: This is just private but should be refactored
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

pub struct FileRepository {
    files: std::collections::HashMap<u32, FileInfo>,
    pub interface: network::Interface,
    next_id: u32,
}

impl FileRepository {
    pub fn new(interface: network::Interface) -> Self {
        return FileRepository {
            files: std::collections::HashMap::new(),
            interface: interface,
            next_id: 0,
        };
    }

    pub fn add_file(&mut self, file: FileInfo) -> Result<ServerTransport> {
        self.files.insert(self.next_id, file);
        return self.interface.addr.make_transport();
    }

    fn get_file(&self, index: u32) -> Result<&FileInfo> {
        return self.files.get(&index)
            .ok_or_else(|| ErrorKind::UnknownFile(index).into());
    }

    pub fn run(&self) -> Result<()> {
        //@Expansion: Maybe don't use fixed ports
        let listener = std::net::TcpListener::bind((self.interface.addr, 2222))
            .chain_err(|| ErrorKind::Bind(self.interface.addr, 2222))?;

        for conn in listener.incoming() {
            let mut stream = conn
                .chain_err(|| ErrorKind::ServerConnection)?;
            //TODO: I should read some sort of info about which file to get here
            let file = self.get_file(0)
                .chain_err(|| ErrorKind::SendFile(stream.peer_addr().unwrap()))?;
            send_file(&mut stream, file)
                .chain_err(|| ErrorKind::SendFile(stream.peer_addr().unwrap()))?;
        }
        return Ok(());
    }
}

pub struct FileClient {
}

impl FileClient{
    pub fn new() -> Self {
        return FileClient {
        }
    }

    pub fn get_file<T: PartialTransport>(&self, transport: T, out_path: Option<std::path::PathBuf>) -> Result<()> {
        let ip = std::net::Ipv4Addr::from_transport(transport)?;
        println!("{} from ip {}",
                 Green.paint("Downloading"),
                 Yellow.paint(ip.to_string()));
        //@Expansion: We can't time out right now. Use the net2::TcpBuilder?
        //@Expansion: Maybe don't use fixed ports
        let stream = std::net::TcpStream::connect((ip, 2222))
            .chain_err(|| ErrorKind::ClientConnection(ip, 2222))?;
        let mut message = FileMessage::read(stream)
            .chain_err(|| ErrorKind::Fetch)?;


        let mut pb = ProgressBar::new(message.size as u64);
        pb.set_units(Units::Bytes);

        let new_path = out_path
            .unwrap_or(std::path::PathBuf::from(&message.name));

        if new_path.exists() {
            bail!(ErrorKind::FileExists(new_path));
        }

        //TODO: Make some error wrapper
        let mut file = std::fs::File::create(new_path)?;

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
}
