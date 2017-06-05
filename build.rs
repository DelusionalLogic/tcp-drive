extern crate byteorder;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::io::BufReader;
use std::io::BufRead;

//@Performance: Pretty slow
//@Memory: Probably a big waste
//@Hack: This should really be in some other datastructure so i wouldn't have to read all of it
fn read_into_vector<P: AsRef<Path>>(path: P) -> Vec<String> {
    let in_file = File::open(path).unwrap();
    let in_buf = BufReader::new(in_file);

    let mut lines = Vec::new();
    for line in in_buf.lines() {
        lines.push(line.unwrap());
    }
    return lines;
}

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("words.rs");
    let mut f = File::create(&dest_path).unwrap();
    let words = read_into_vector("./newwords.txt");

    f.write_all(b"
    fn make_list() -> Box<[&'static str]> {
        return Box::new([");
    for word in &words {
        write!(f, "\"{}\",\n", word).unwrap();
    }
    f.write_all(b"
    ]);}");
}
