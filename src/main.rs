#[macro_use]
extern crate log;
extern crate clap;

mod network;

use std::path::PathBuf;
use clap::App;
use clap::SubCommand;
use clap::Arg;
use std::io::BufRead;
use std::io::{Read, Write};

//@Memory: LARGE
const WORDS : &'static str = include_str!("words.txt");

//@Performance: Pretty slow
//@Memory: Probably a big waste
//@Hack: This should really be in some other datastructure so i wouldn't have to read all of it
fn read_into_vector(string: &'static str) -> Result<Vec<String>, std::io::Error> {
    info!("Reading words into array");
    let cursor = std::io::Cursor::new(string);
    let mut lines = Vec::new();
    for line in cursor.lines() {
        lines.push(try!(line))
    }
    debug!("Read {} words", lines.len());
    return Ok(lines);
}

type Dict = Vec<String>;

trait Transportable {
    fn make_transport(&self, dict: &Dict) -> String;
    fn from_transport<S: Into<String>>(dict: &Dict, transport: S) -> Self;
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

    fn from_transport<S: Into<String>>(dict: &Dict, transport: S) -> Self {
        let transport : String = transport.into();
        let ip_vec = transport
              .split(" ")
              .map(| t | dict.binary_search(&t.to_owned()).unwrap() as u32) //@Error: Make error massage
              .collect::<Vec<u32>>();

        return std::net::Ipv4Addr::from(ip_vec[1] |  ip_vec[0] << 16);
    }
}

fn send_file(stream: &mut std::net::TcpStream, path: &PathBuf) {
    //@Error: This shouldn't happen here
    let mut file = std::fs::File::open(path).unwrap();
    let mut buffer = [0u8; 512];
    //@Error: Improper on file read failed
    loop{
        let read = file.read(&mut buffer).expect("Failed reading file");
        let write = stream.write(&buffer[0..read]).expect("Failed writing to stream");
        if read == 0 || write == 0 {
            break;
        }
    }
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
                         .value_name("KEY")
                         .help("Key of remote file")
                        )
                    ).get_matches();

    //@Error: Make error
    let lines = read_into_vector(WORDS).unwrap();

    if let Some(matches) = matches.subcommand_matches("serve") {
        //We know that file has to be provided
        let path = PathBuf::from(matches.value_of("file").unwrap());
        //@Error: Write something better
        let port : u16 = matches
            .value_of("port")
            .unwrap_or("2222")
            .parse()
            .expect("Failed parsing the port number");
        info!("Serving file: \"{}\"", path.to_str().unwrap());

        //@Error: Make error
        let interfaces = network::get_interfaces().unwrap();

        //@Error: Proper errors
        let listener = std::net::TcpListener::bind(("0.0.0.0", port)).expect("Hello");

        for interface in &interfaces {
            println!("Name {}, ip: {}, transport: {}", interface.name, interface.addr, interface.addr.make_transport(&lines));
        }

        for conn in listener.incoming() {
            match conn {
                Ok(mut stream) => send_file(&mut stream, &path),
                Err(e) => panic!(e),
            }
        }
    }

    if let Some(matches) = matches.subcommand_matches("fetch") {
        let key = matches.value_of("key").unwrap();

        let ip = std::net::Ipv4Addr::from_transport(&lines, key);
        println!("Decoded ip {}", ip);

        //@Error: Proper errors
        let mut stream = std::net::TcpStream::connect((ip, 2222)).expect("Failed to connect");
        //@Error: Proper errors
        let mut file = std::fs::File::create("Testfile.txt").unwrap();
        let mut buffer = [0u8; 512];
        loop{
            let read = stream.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            let write = file.write(&mut buffer[0..read]).unwrap();
        }
    }
    return;
}
