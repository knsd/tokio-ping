#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "too small header")]
    TooSmallHeader,
    #[fail(display = "invalid header size")]
    InvalidHeaderSize,
    #[fail(display = "invalid version")]
    InvalidVersion,
    #[fail(display = "unknown protocol")]
    UnknownProtocol,
}

const MINIMUM_PACKET_SIZE: usize = 20;

#[derive(Debug, PartialEq)]
pub enum IpV4Protocol {
    Icmp,
}

impl IpV4Protocol {
    fn decode(data: u8) -> Option<Self> {
        match data {
            1 => Some(IpV4Protocol::Icmp),
            _ => None,
        }
    }
}

pub struct IpV4Packet<'a> {
    pub protocol: IpV4Protocol,
    pub data: &'a [u8],
}

impl<'a> IpV4Packet<'a> {
    pub fn decode(data: &'a [u8]) -> Result<Self, Error> {
        if data.len() < MINIMUM_PACKET_SIZE {
            return Err(Error::TooSmallHeader);
        }
        let byte0 = data[0];
        let version = (byte0 & 0xf0) >> 4;
        let header_size = 4 * ((byte0 & 0x0f) as usize);

        if version != 4 {
            return Err(Error::InvalidVersion);
        }

        if data.len() < header_size {
            return Err(Error::InvalidHeaderSize);
        }

        let protocol = match IpV4Protocol::decode(data[9]) {
            Some(protocol) => protocol,
            None => return Err(Error::UnknownProtocol),
        };

        Ok(Self {
            protocol: protocol,
            data: &data[header_size..],
        })
    }
}
