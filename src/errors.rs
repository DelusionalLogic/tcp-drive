use std::error;
use network;
use std::io;
use std::fmt;
use std::net;
use std::path;

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

impl error::Error for SendError {
    fn description(&self) -> &str {
        match *self {
            SendError::Io(_) => "An I/O error occured during serving",
            SendError::Serialization(_) => "An error occured while serializing and sending the message",
            SendError::PathConversion => "An error occured while converting the path to a string",
        }
    }

    fn cause(&self) -> Option<&error::Error> {
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

impl error::Error for ServeError {
    fn description(&self) -> &str {
        match *self {
            ServeError::Io(_) => "An I/O error occured during serving",
            ServeError::Connection(_) => "A low level error occured while processing connection",
            ServeError::Enumeration(_) => "While enumerating interfaces",
            ServeError::Bind(_, _, _) => "Error while binding connection",
            ServeError::SendingFile(_) => "Error while sending a file",
        }
    }

    fn cause(&self) -> Option<&error::Error> {
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

impl error::Error for SerializationError {
    fn description(&self) -> &str {
        match *self {
            SerializationError::Io(_) => "An I/O error occured during serving",
            SerializationError::IncompleteRead(_, _) =>
                "An error occured which caused a read to end before getting the expected data",
        }
    }

    fn cause(&self) -> Option<&error::Error> {
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
    FileExists(path::PathBuf),

    Io(io::Error),
    Connection(io::Error, net::Ipv4Addr, u16),
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

impl error::Error for FetchError {
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

    fn cause(&self) -> Option<&error::Error> {
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

