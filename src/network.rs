extern crate libc;

use std;
use std::error::Error;
use std::fmt;
use std::net::Ipv4Addr;
use std::mem;
use std::io;
use std::ffi;

#[derive(Debug)]
pub enum NetworkError {
    Io(io::Error),
    INet(i32),
    Str(ffi::IntoStringError),
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            NetworkError::Io(ref err) => write!(f, "IO Error {}", err),
            NetworkError::INet(ref errno) => write!(f, "INet Error {}", errno),
            NetworkError::Str(ref err) => write!(f, "Failed to convert a string {}", err),
        }
    }
}

impl Error for NetworkError {
    fn description(&self) -> &str {
        match *self {
            NetworkError::Io(ref err) => err.description(),
            NetworkError::INet(_) => "A Network error occured",
            NetworkError::Str(ref err) => err.description(),
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            NetworkError::Io(ref err) => Some(err),
            NetworkError::INet(_) => None,
            NetworkError::Str(ref err) => Some(err),
        }
    }
}

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

pub fn interfaces() -> Result<Vec<Interface>, NetworkError> {
    info!("Getting interfaces");
    let mut interfaces = Vec::new();
    let mut addrs : *mut libc::ifaddrs = unsafe{ mem::uninitialized() };

    if unsafe { libc::getifaddrs(&mut addrs) != 0 } { 
        //Error
        return Err(NetworkError::Io(io::Error::last_os_error()));
    }

    let mut thisaddr = addrs;
    loop {
        let sock_size = std::mem::size_of::<libc::sockaddr_in>() as u32;
        //@Expansion: Only supports ipv4, i think
        let mut hostname = Vec::with_capacity(128);
        let family = unsafe{ (*(*thisaddr).ifa_addr).sa_family };
        //@Expansion: Only supports ipv4 right now. Ignores everything else
        if family == libc::AF_INET as u16 {
            //Lookup the name of the address. Only returns 0 if the interface is connected
            let addr_info_ret = unsafe{ libc::getnameinfo((*thisaddr).ifa_addr, sock_size, hostname.as_mut_ptr(), hostname.capacity() as u32, std::ptr::null::<i8>() as *mut i8, 0, 1) };
            if addr_info_ret == 0 {

                //@Memory: I don't know if this is correct
                //@Leak: Might leak
                let name = unsafe{ ffi::CString::from_raw((*thisaddr).ifa_name) };
                let data = unsafe{ (*(*thisaddr).ifa_addr).sa_data };
                let addr = Ipv4Addr::new(data[2] as u8, data[3] as u8, data[4] as u8, data[5] as u8);

                let interface = Interface {
                    name: try!(name.into_string()),
                    addr: addr,
                };

                interfaces.push(interface);
            } else if addr_info_ret != -3 { //@Hack: EAI_AGAIN is defined to -3 i think, but might not be
                unsafe{ libc::freeifaddrs(addrs) } //Remember to free
                return Err(NetworkError::INet(addr_info_ret)); //@Think: Maybe this should be a different type?
            }
        }

        thisaddr = unsafe{ (*thisaddr).ifa_next };
        if thisaddr.is_null() {
            break;
        }
    }
    unsafe{ libc::freeifaddrs(addrs) }
    return Ok(interfaces);
}
