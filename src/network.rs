extern crate libc;

use std;
use std::error::Error;
use std::fmt;
use std::net::Ipv4Addr;
use std::mem;
use std::io;
use std::ffi;
use std::ptr;

#[derive(Debug)]
pub enum NetworkError {
    Io(io::Error),
    Str(ffi::IntoStringError),
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            NetworkError::Io(ref err) => write!(f, "IO Error {}", err),
            NetworkError::Str(ref err) => write!(f, "Failed to convert a string {}", err),
        }
    }
}

impl Error for NetworkError {
    fn description(&self) -> &str {
        match *self {
            NetworkError::Io(ref err) => err.description(),
            NetworkError::Str(ref err) => err.description(),
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            NetworkError::Io(ref err) => Some(err),
            NetworkError::Str(ref err) => Some(err),
        }
    }
}

/*
impl From<io::Error> for CliError {
    fn from(err: io::Error) -> CliError {
        CliError::Io(err)
    }
}
*/

impl From<io::Error> for NetworkError {
    fn from(err: io::Error) -> NetworkError {
        NetworkError::Io(err)
    }
}

impl From<ffi::IntoStringError> for NetworkError {
    fn from(err: ffi::IntoStringError) -> NetworkError {
        NetworkError::Str(err)
    }
}

pub struct Interface {
    pub name : String,
    pub addr : Ipv4Addr,
}

pub fn getInterfaces() -> Result<Vec<Interface>, NetworkError> {
    let mut interfaces = Vec::new();
    let mut addrs : *mut libc::ifaddrs = unsafe{ mem::uninitialized() };

    if unsafe { libc::getifaddrs(&mut addrs) != 0 } { 
        //Error
        return Err(NetworkError::Io(io::Error::last_os_error()));
    }

    let mut thisaddr = addrs;
    loop {
        let sockSize = std::mem::size_of::<libc::sockaddr_in>() as u32;
        let mut hostname = Vec::with_capacity(128);
        if unsafe{ libc::getnameinfo((*thisaddr).ifa_addr, sockSize, hostname.as_mut_ptr(), hostname.capacity() as u32, std::ptr::null::<i8>() as *mut i8, 0, 1) } == 0 {
            let name = unsafe{ ffi::CString::from_raw((*thisaddr).ifa_name) };
            let data = unsafe{ (*(*thisaddr).ifa_addr).sa_data };
            let addr = Ipv4Addr::new(data[2] as u8, data[3] as u8, data[4] as u8, data[5] as u8);
            let interface = Interface {
                name: try!(name.into_string()),
                addr: addr,
            };
            interfaces.push(interface);
        }
        thisaddr = unsafe{ (*thisaddr).ifa_next };
        if thisaddr.is_null() {
            break;
        }
    }
    unsafe{ libc::freeifaddrs(addrs) }
    return Ok(interfaces);
}
