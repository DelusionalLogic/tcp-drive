#[macro_use]
extern crate log;
extern crate clap;
extern crate byteorder;
extern crate ansi_term;
extern crate pbr;
#[macro_use]
extern crate send;

mod network;

use std::path::PathBuf;
use clap::App;
use clap::SubCommand;
use clap::Arg;
use std::error::Error;
use std::io;
use std::fmt;
use ansi_term::Colour::*;
use send::Transportable;
use send::TransportPresenter;
use send::errors::*;

#[derive(Debug)]
pub enum AppError {
    Io(io::Error),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            AppError::Io(_) => write!(f, "An IO error occured"),
        }
    }
}

impl Error for AppError {
    fn description(&self) -> &str {
        match *self {
            AppError::Io(ref err) => err.description(),
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            AppError::Io(ref err) => Some(err),
        }
    }
}

impl From<io::Error> for AppError {
    fn from(err: io::Error) -> AppError {
        AppError::Io(err)
    }
}

fn print_interface(presenter: &TransportPresenter, interface: &network::Interface) {
    println!("{}[{}]\n {} {}",
             Green.paint(interface.name.to_string()),
             Yellow.paint(interface.addr.to_string()),
             Blue.paint("=>"),
             presenter.present(&interface.addr).unwrap()
            );
}

//@Refactor: Move file opening and duplicate detection somewhere else?
//@MEMORY @SPEED This has a real slow and real bad generated function. It's terrible!
include!(concat!(env!("OUT_DIR"), "/words.rs"));

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

    let (glob_lines, glob_count) = make_list();
    let presenter = TransportPresenter::new(glob_lines, glob_count);

    if let Some(matches) = matches.subcommand_matches("serve") {
        //We know that file has to be provided
        let path = PathBuf::from(matches.value_of("file").unwrap());

        //@Error: Write something better
        let port : u16 = matches
            .value_of("port")
            .unwrap_or("2222")
            .parse()
            .expect("Failed parsing the port number");

        let file = send::FileInfo::from_path(path)
            .expect("Failed opening file");


        let interfaces = network::interfaces().unwrap();
        for interface in &interfaces {
            info!("Interface: {}", interface.name);
            print_interface(&presenter, &interface);
        }
        if let Err(err) = send::serve_file(file, port) {
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

        if let Err(err) = send::fetch_file(presenter, key, new_path) {
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
