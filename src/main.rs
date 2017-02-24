#[macro_use]
extern crate log;
extern crate clap;
extern crate byteorder;
extern crate ansi_term;
extern crate pbr;

mod network;

use std::path::PathBuf;
use clap::App;
use clap::SubCommand;
use clap::Arg;
use std::io::BufRead;
use std::io::{Read, Write};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use std::error::Error;
use std::io;
use std::fmt;
use ansi_term::Colour::*;
use std::cmp::Ordering;
use pbr::{ProgressBar, Units};

#[derive(Debug)]
pub enum SerializationError {
    Io(io::Error),
    IncompleteRead(usize, usize),
}

impl fmt::Display for SerializationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            SerializationError::Io(_) => write!(f, "An IO error occured"),
            SerializationError::IncompleteRead(actual, expected) =>
                write!(f, "A read didn't get the expected amount of data [Expected {}, Actual {}]", actual, expected),
        }
    }
}

impl Error for SerializationError {
    fn description(&self) -> &str {
        match *self {
            SerializationError::Io(_) => "An I/O error occured during serving",
            SerializationError::IncompleteRead(_, _) =>
                "An error occured which caused a read to end before getting the expected data",
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            SerializationError::Io(ref err) => Some(err),
            SerializationError::IncompleteRead(_, _) => None,
        }
    }
}

impl From<io::Error> for SerializationError {
    fn from(err: io::Error) -> SerializationError {
        SerializationError::Io(err)
    }
}

#[derive(Debug)]
pub enum FetchError {
    InvalidTransport(String),
    FileExists(PathBuf),

    Io(io::Error),
    Connection(io::Error, std::net::Ipv4Addr, u16),
    ReadMessage(SerializationError),
    ReadContent(io::Error),
    WriteContent(io::Error),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            FetchError::InvalidTransport(ref word) => write!(f, "The transport \"{}\" isn't valid", word),
            FetchError::FileExists(ref path) => write!(f, "File already exists: {}, that could have been bad", path.to_string_lossy()),

            FetchError::Io(_) => write!(f, "An IO error occured"),
            FetchError::Connection(_, ip, port) => write!(f, "While connecting to {}:{}", ip, port),
            FetchError::ReadMessage(_) => write!(f, "While reading file message from network"),
            FetchError::ReadContent(_) => write!(f, "While reading content of file from network"),
            FetchError::WriteContent(_) => write!(f, "While writing content of file to disk"),
        }
    }
}

impl Error for FetchError {
    fn description(&self) -> &str {
        match *self {
            FetchError::InvalidTransport(_) => "The given transport was invalid",
            FetchError::FileExists(_) => "The specified file already exists",

            FetchError::Io(_) => "An I/O error occured during serving",
            FetchError::Connection(_, _, _) => "A connection error occured",
            FetchError::ReadMessage(_) => "An error occured while reading the file messages from the network",
            FetchError::ReadContent(_) => "An error occured while reading the file cotents from the network",
            FetchError::WriteContent(_) => "An error occured while writing the file cotents to the network",
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            FetchError::InvalidTransport(_) => None,
            FetchError::FileExists(_) => None,

            FetchError::Io(ref err) => Some(err),
            FetchError::Connection(ref err, _, _) => Some(err),
            FetchError::ReadMessage(ref err) => Some(err),
            FetchError::ReadContent(ref err) => Some(err),
            FetchError::WriteContent(ref err) => Some(err),
        }
    }
}

impl From<io::Error> for FetchError {
    fn from(err: io::Error) -> FetchError {
        FetchError::Io(err)
    }
}

#[derive(Debug)]
pub enum SendError {
    Io(io::Error),
    Serialization(SerializationError),
    PathConversion,
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            SendError::Io(_) => write!(f, "An IO error occured"),
            SendError::Serialization(_) => write!(f, "While sending the message"),
            SendError::PathConversion => write!(f, "Failed converting the path to a string"),
        }
    }
}

impl Error for SendError {
    fn description(&self) -> &str {
        match *self {
            SendError::Io(_) => "An I/O error occured during serving",
            SendError::Serialization(_) => "An error occured while serializing and sending the message",
            SendError::PathConversion => "An error occured while converting the path to a string",
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            SendError::Io(ref err) => Some(err),
            SendError::Serialization(ref err) => Some(err),
            SendError::PathConversion => None,
        }
    }
}

impl From<io::Error> for SendError {
    fn from(err: io::Error) -> SendError {
        SendError::Io(err)
    }
}

#[derive(Debug)]
pub enum ServeError {
    Io(io::Error),
    Connection(io::Error),
    Enumeration(network::NetworkError),
    Bind(io::Error, &'static str, u16), //@Expansion: We might need this to not be a static str
    SendingFile(SendError),
}

impl fmt::Display for ServeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ServeError::Io(_) => write!(f, "An IO error occured"),
            ServeError::Connection(_) => write!(f, "While processing connection"),
            ServeError::Enumeration(_) => write!(f, "While enumerating interfaces"),
            ServeError::Bind(_, ip, port) => write!(f, "While binding to {}:{}", ip, port),
            ServeError::SendingFile(_) => write!(f, "While sending file"),
        }
    }
}

impl Error for ServeError {
    fn description(&self) -> &str {
        match *self {
            ServeError::Io(_) => "An I/O error occured during serving",
            ServeError::Connection(_) => "A low level error occured while processing connection",
            ServeError::Enumeration(_) => "While enumerating interfaces",
            ServeError::Bind(_, _, _) => "Error while binding connection",
            ServeError::SendingFile(_) => "Error while sending a file",
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            ServeError::Io(ref err) => Some(err),
            ServeError::Connection(ref err) => Some(err),
            ServeError::Enumeration(ref err) => Some(err),
            ServeError::Bind(ref err, _, _) => Some(err),
            ServeError::SendingFile(ref err) => Some(err),
        }
    }
}

impl From<io::Error> for ServeError {
    fn from(err: io::Error) -> ServeError {
        ServeError::Io(err)
    }
}

#[derive(Debug)]
pub enum AppError {
    Io(io::Error),
    Serve(ServeError),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            AppError::Io(_) => write!(f, "An IO error occured"),
            AppError::Serve(_) => write!(f, "An error occured while serving file"),
        }
    }
}

impl Error for AppError {
    fn description(&self) -> &str {
        match *self {
            AppError::Io(ref err) => err.description(),
            AppError::Serve(_) => "An error occured during file serving",
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            AppError::Io(ref err) => Some(err),
            AppError::Serve(ref err) => Some(err),
        }
    }
}

impl From<io::Error> for AppError {
    fn from(err: io::Error) -> AppError {
        AppError::Io(err)
    }
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

//@Memory: LARGE
const WORDS : &'static str = include_str!("words.txt");

type Dict = Vec<String>;

//@Performance: Pretty slow
//@Memory: Probably a big waste
//@Hack: This should really be in some other datastructure so i wouldn't have to read all of it
fn read_into_vector(string: &'static str) -> Result<Dict, AppError> {
    info!("Reading words into array");
    let cursor = std::io::Cursor::new(string);
    let mut lines = Vec::new();
    for line in cursor.lines() {
        lines.push(try!(line))
    }
    debug!("Read {} words", lines.len());
    return Ok(lines);
}

trait Transportable {
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

macro_rules! atry {
    ($expr:expr, $map:expr) => (
        try!($expr.map_err($map))
    );
}

struct FileInfo{
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

    fn from_path(path: PathBuf) -> Result<FileInfo, ServeError> {
        let metadata = try!(std::fs::metadata(&path));
        return Ok(FileInfo::new(path, metadata.len()))
    }

    fn open(&self) -> Result<std::fs::File, std::io::Error> {
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

fn print_interface(lines: &Dict, interface: &network::Interface) {
    println!("{}[{}]\n {} {}",
             Green.paint(interface.name.to_string()),
             Yellow.paint(interface.addr.to_string()),
             Blue.paint("=>"),
             interface.addr.make_transport(&lines)
            );
}

//@Refactor: Move file opening and duplicate detection somewhere else?
fn serve_file(lines: &Dict, file: FileInfo, port: u16) -> Result<(), ServeError> {
        let interfaces = atry!(network::interfaces(), ServeError::Enumeration);

        let listener = atry!(std::net::TcpListener::bind(("0.0.0.0", port)), | err | ServeError::Bind(err, "0.0.0.0", port));

        for interface in &interfaces {
            info!("Interface: {}", interface.name);
            print_interface(lines, &interface);
        }

        for conn in listener.incoming() {
            let mut stream = atry!(conn, ServeError::Connection);

            atry!(send_file(&mut stream, &file), ServeError::SendingFile);
        }
        return Ok(());
}

fn fetch_file(lines: &Dict, key: String, file: Option<std::path::PathBuf>) ->  Result<(), FetchError> {
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

fn main() {
    let matches = App::new("Send")
        .version("1.0")
        .author("Jesper Jensen")
        .about("A program to send files")
        .subcommand(SubCommand::with_name("serve")
                    .about("Serve a file")
                    .arg(Arg::with_name("file")
                         .index(1)
                         .required(true)
                         .multiple(false)
                         .value_name("FILE")
                         .help("File to serve")
                        )
                    .arg(Arg::with_name("port")
                         .short("p")
                         .long("port")
                         .value_name("PORT")
                         .help("Port to send on")
                        )
                    )
        .subcommand(SubCommand::with_name("fetch")
                    .about("Fetch a file")
                    .arg(Arg::with_name("key")
                         .index(1)
                         .required(true)
                         .multiple(true)
                         .value_name("KEY")
                         .help("Key of remote file")
                        )
                    .arg(Arg::with_name("file")
                         .short("f")
                         .long("file")
                         .value_name("FILE")
                         .help("Filename of the new file")
                        )
                    ).get_matches();

    //@Error: Make error
    let lines = read_into_vector(WORDS)
        .unwrap_or_else( | err | panic!("{}", err));

    if let Some(matches) = matches.subcommand_matches("serve") {
        //We know that file has to be provided
        let path = PathBuf::from(matches.value_of("file").unwrap());

        //@Error: Write something better
        let port : u16 = matches
            .value_of("port")
            .unwrap_or("2222")
            .parse()
            .expect("Failed parsing the port number");

        let file = FileInfo::from_path(path)
            .expect("Failed opening file");

        if let Err(err) = serve_file(&lines, file, port) {
            println!(" {} {}", Red.paint("==>"), err);
            let mut terr : &std::error::Error = &err;
            while let Some(serr) = terr.cause() {
                println!("    {} {}", Yellow.paint("==>"), serr);
                terr = serr;
            }
        }
    } else if let Some(matches) = matches.subcommand_matches("fetch") {
        //There has to be a key for the commandline to be valid so just unwrap
        let key = matches.values_of("key").unwrap()
            .collect::<Vec<_>>()
            .join(" ");
        let new_path = matches.value_of("file")
            .map(| path | std::path::PathBuf::from(path));

        if let Err(err) = fetch_file(&lines, key, new_path) {
            println!(" {} {}", Red.paint("==>"), err);
            let mut terr : &std::error::Error = &err;
            while let Some(serr) = terr.cause() {
                println!("    {} {}", Yellow.paint("==>"), serr);
                terr = serr;
            }
        }
    }
    return;
}
