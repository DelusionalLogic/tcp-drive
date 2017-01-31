#[macro_use]
extern crate log;
extern crate clap;

mod network;

use std::env;
use std::path::PathBuf;
use clap::App;
use clap::SubCommand;
use clap::Arg;


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
                         .multiple(true)
                         .value_name("FILE")
                         .help("File to serve")
                         )
                    ).get_matches();

    if let Some(matches) = matches.subcommand_matches("serve") {
        info!("Serving files");
        let interfaces = network::getInterfaces().unwrap();
        for interface in &interfaces {
            println!("Name {}, ip: {}", interface.name, interface.addr);
        }
    }
}
