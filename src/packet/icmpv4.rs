mod errors {
    error_chain! {
        errors {
            EmptyData
            InvalidHeaderDataSize
            InvalidHeaderSize
        }
    }
}

const ICMPV4_HEADER_SIZE: usize = 8;
const ICMPV4_HEADER_DATA_SIZE: usize = 4;

const ECHO_REQUEST: u8 = 8;
const ECHO_REPLY: u8 = 0;

#[derive(Debug)]
pub struct IcmpV4Header<Data> {
    kind: u8,
    code: u8,
    checksum: u16,
    data: Data,
}

impl<Data: IcmpV4HeaderData> IcmpV4Header<Data> {
    fn encode(&self) -> [u8; ICMPV4_HEADER_SIZE] {
        let mut buf = [0; ICMPV4_HEADER_SIZE];

        buf[0] = self.kind;
        buf[1] = self.code;
        buf[2] = (self.checksum >> 8) as u8;
        buf[3] = self.checksum as u8;

        let data_buf = self.data.encode();

        buf[4] = data_buf[0];
        buf[5] = data_buf[1];
        buf[6] = data_buf[2];
        buf[7] = data_buf[3];

        buf
    }

    fn decode(data: &[u8]) -> errors::Result<Self> {
        if data.len() == ICMPV4_HEADER_SIZE {
            let kind = data[0];
            let code = data[1];
            let checksum = (u16::from(data[2]) << 8) + u16::from(data[3]);
            let data = Data::decode(&data[4..])?;
            Ok(Self {
                kind: kind,
                code: code,
                checksum: checksum,
                data: data,
            })
        } else {
            Err(errors::ErrorKind::InvalidHeaderSize.into())
        }
    }
}

pub trait IcmpV4HeaderData: Sized {
    fn encode(&self) -> [u8; ICMPV4_HEADER_DATA_SIZE];
    fn decode(data: &[u8]) -> errors::Result<Self>;
}

#[derive(Debug)]
pub struct IdentSeqData {
    ident: u16,
    seq_cnt: u16,
}

impl IcmpV4HeaderData for IdentSeqData {
    fn encode(&self) -> [u8; ICMPV4_HEADER_DATA_SIZE] {
        let mut buf = [0; ICMPV4_HEADER_DATA_SIZE];

        buf[0] = (self.ident >> 8) as u8;
        buf[1] = self.ident as u8;
        buf[2] = (self.seq_cnt >> 8) as u8;
        buf[3] = self.seq_cnt as u8;

        buf
    }

    fn decode(data: &[u8]) -> errors::Result<Self> {
        if data.len() == ICMPV4_HEADER_DATA_SIZE {
            let ident = (u16::from(data[0]) << 8) + u16::from(data[1]);
            let seq_cnt = (u16::from(data[2]) << 8) + u16::from(data[3]);

            Ok(Self {
                ident: ident,
                seq_cnt: seq_cnt,
            })
        } else {
            Err(errors::ErrorKind::InvalidHeaderDataSize.into())
        }
    }
}

#[derive(Debug)]
pub struct RawData {
    inner: [u8; ICMPV4_HEADER_DATA_SIZE],
}

impl IcmpV4HeaderData for RawData {
    fn encode(&self) -> [u8; ICMPV4_HEADER_DATA_SIZE] {
        self.inner
    }

    fn decode(data: &[u8]) -> errors::Result<Self> {
        if data.len() == ICMPV4_HEADER_DATA_SIZE {
            Ok(Self {
                inner: [data[0], data[1], data[2], data[3]],
            })
        } else {
            Err(errors::ErrorKind::InvalidHeaderDataSize.into())
        }
    }
}

#[derive(Debug)]
pub struct RawIcmpV4Message<'a, Data> {
    pub header: IcmpV4Header<Data>,
    pub payload: &'a [u8],
}

impl<'a, Data: IcmpV4HeaderData> RawIcmpV4Message<'a, Data> {
    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(ICMPV4_HEADER_SIZE + self.payload.len());
        buf.extend_from_slice(&self.header.encode());
        buf.extend_from_slice(self.payload);

        let mut sum = 0u32;
        for word in buf.chunks(2) {
            let mut part = u16::from(word[0]) << 8;
            if word.len() > 1 {
                part += u16::from(word[1]);
            }
            sum = sum.wrapping_add(u32::from(part));
        }

        while (sum >> 16) > 0 {
            sum = (sum & 0xffff) + (sum >> 16);
        }

        let sum = !sum as u16;

        buf[2] = (sum >> 8) as u8;
        buf[3] = (sum & 0xff) as u8;

        buf
    }

    pub fn decode(data: &'a [u8]) -> errors::Result<Self> {
        let header = IcmpV4Header::decode(&data[..ICMPV4_HEADER_SIZE])?;
        Ok(Self {
            header: header,
            payload: &data[ICMPV4_HEADER_SIZE..],
        })
    }
}

#[derive(Debug)]
pub enum IcmpV4Message<'a> {
    EchoReply(RawIcmpV4Message<'a, IdentSeqData>),
    EchoRequest(RawIcmpV4Message<'a, IdentSeqData>),
    Unknown(RawIcmpV4Message<'a, RawData>),
}

impl<'a> IcmpV4Message<'a> {
    pub fn echo_request(ident: u16, seq_cnt: u16, payload: &'a [u8]) -> IcmpV4Message {
        let header = IcmpV4Header {
            kind: ECHO_REQUEST,
            code: 0,
            checksum: 0,
            data: IdentSeqData {
                ident: ident,
                seq_cnt: seq_cnt,
            }
        };
        IcmpV4Message::EchoRequest(RawIcmpV4Message {
            header: header,
            payload: payload,
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        match *self {
            IcmpV4Message::EchoReply(ref inner) => inner.encode(),
            IcmpV4Message::EchoRequest(ref inner) => inner.encode(),
            IcmpV4Message::Unknown(ref inner) => inner.encode(),
        }
    }

    pub fn decode(data: &'a [u8]) -> errors::Result<Self> {
        Ok(match data.get(0).cloned() {
            Some(ECHO_REPLY) => IcmpV4Message::EchoReply(RawIcmpV4Message::decode(data)?),
            Some(ECHO_REQUEST) => IcmpV4Message::EchoRequest(RawIcmpV4Message::decode(data)?),
            Some(_) => IcmpV4Message::Unknown(RawIcmpV4Message::decode(data)?),
            None => return Err(errors::ErrorKind::EmptyData.into())
        })
    }
}
