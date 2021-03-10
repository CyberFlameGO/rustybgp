// Copyright (C) 2019-2020 The RustyBGP Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
// implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::error::Error;
use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use std::collections::HashSet;
use std::convert::From;
use std::fmt;
use std::io::{Cursor, Read, Write};
use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;

pub const BGP_PORT: u16 = 179;

const AS_TRANS: u16 = 23456;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IpNet {
    pub addr: IpAddr,
    pub mask: u8,
}

impl IpNet {
    pub fn is_v6(&self) -> bool {
        self.addr.is_ipv6()
    }

    pub fn from_bytes(c: &mut Cursor<&[u8]>, is_v6: bool) -> Result<IpNet, Error> {
        let bit_len = c.read_u8()?;
        if is_v6 {
            let mut addr = [0_u8; 16];
            for i in 0..(bit_len + 7) / 8 {
                addr[i as usize] = c.read_u8()?;
            }
            Ok(IpNet::new(addr, bit_len))
        } else {
            let mut addr = [0_u8; 4];
            for i in 0..(bit_len + 7) / 8 {
                addr[i as usize] = c.read_u8()?;
            }
            Ok(IpNet::new(addr, bit_len))
        }
    }

    pub fn contains(&self, addr: IpAddr) -> bool {
        let f = |a: &Vec<u8>, b: &Vec<u8>, mask: u8| {
            let div = (mask >> 3) as usize;

            for i in 0..div {
                if a[i] != b[i] {
                    return false;
                }
            }

            let r = mask & 0x07;
            if r > 0 {
                let bit = 8 - r;
                if a[div] != (b[div] & (0xff >> bit << bit)) {
                    return false;
                }
            }
            true
        };
        match addr {
            IpAddr::V4(addr) => {
                let prefix_octets: Vec<u8> = match self.addr {
                    IpAddr::V4(addr) => addr.octets().iter().copied().collect(),
                    _ => return false,
                };

                let addr_octests: Vec<u8> = addr.octets().iter().copied().collect();
                f(&prefix_octets, &addr_octests, self.mask)
            }
            IpAddr::V6(addr) => {
                let prefix_octets: Vec<u8> = match self.addr {
                    IpAddr::V6(addr) => addr.octets().iter().copied().collect(),
                    _ => return false,
                };

                let addr_octests: Vec<u8> = addr.octets().iter().copied().collect();
                f(&prefix_octets, &addr_octests, self.mask)
            }
        }
    }

    fn clear_bits(buf: &mut [u8], mask: u8) {
        let rem = mask % 8;
        if rem != 0 {
            let prefix_len = (mask + 7) / 8;
            let mask: u16 = 0xff00 >> rem;
            buf[prefix_len as usize - 1] &= mask as u8;
        }
    }

    fn size(&self) -> usize {
        (1 + (self.mask + 7) / 8) as usize
    }

    fn to_bytes(&self, c: &mut Cursor<Vec<u8>>) -> Result<usize, Error> {
        let pos = c.position();
        let prefix_len = (self.mask + 7) / 8;

        c.write_u8(self.mask)?;
        match self.addr {
            IpAddr::V4(addr) => {
                for i in 0..prefix_len {
                    c.write_u8(addr.octets()[i as usize])?;
                }
            }

            IpAddr::V6(addr) => {
                for i in 0..prefix_len {
                    c.write_u8(addr.octets()[i as usize])?;
                }
            }
        }
        Ok((c.position() - pos) as usize)
    }
}

impl FromStr for IpNet {
    type Err = Error;

    fn from_str(s: &str) -> Result<IpNet, Error> {
        let addr_and_mask: Vec<_> = s.split('/').collect();
        if addr_and_mask.len() != 2 {
            return Err(Error::InvalidFormat);
        }

        let addr = IpAddr::from_str(addr_and_mask[0]);
        let addr = match addr {
            Ok(addr) => addr,
            Err(_) => {
                return Err(Error::InvalidFormat);
            }
        };

        let mask = u8::from_str(addr_and_mask[1]);
        let mask = match mask {
            Ok(mask) => mask,
            Err(_) => {
                return Err(Error::InvalidFormat);
            }
        };

        match addr {
            IpAddr::V4(addr) => {
                if mask > 32 {
                    return Err(Error::InvalidFormat);
                }
                Ok(IpNet::new(addr.octets(), mask))
            }
            IpAddr::V6(addr) => {
                if mask > 128 {
                    return Err(Error::InvalidFormat);
                }
                Ok(IpNet::new(addr.octets(), mask))
            }
        }
    }
}

#[test]
fn from_str_ipnet() {
    assert_eq!(
        IpNet::from_str("1.1.1.0/24").unwrap(),
        IpNet::new([1, 1, 1, 0], 24),
    );
}

impl fmt::Display for IpNet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.addr, self.mask)
    }
}

pub trait IpNetNew<T>: Sized {
    #[allow(clippy::new_ret_no_self)]
    fn new(_: T, mask: u8) -> IpNet;
}

impl IpNetNew<[u8; 4]> for IpNet {
    fn new(mut octets: [u8; 4], mask: u8) -> IpNet {
        IpNet::clear_bits(&mut octets, mask);
        IpNet {
            addr: IpAddr::from(octets),
            mask,
        }
    }
}

impl IpNetNew<[u8; 16]> for IpNet {
    fn new(mut octets: [u8; 16], mask: u8) -> IpNet {
        IpNet::clear_bits(&mut octets, mask);
        IpNet {
            addr: IpAddr::from(octets),
            mask,
        }
    }
}

#[test]
fn ipnet_oddbits() {
    let mut octests = [1, 0xff, 0xff, 0];
    IpNet::clear_bits(&mut octests, 23);
    assert_eq!(octests[2], 0xfe);
}

#[test]

fn ipnet_contains() {
    use std::net::Ipv6Addr;

    let n1 = IpNet::from_str("2.2.2.0/24").unwrap();
    assert_eq!(n1.contains(IpAddr::V4(Ipv4Addr::new(2, 2, 2, 3))), true);
    assert_eq!(n1.contains(IpAddr::V4(Ipv4Addr::new(2, 2, 1, 3))), false);
    assert_eq!(
        n1.contains(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1))),
        false
    );

    let n2 = IpNet::from_str("2.2.2.128/25").unwrap();
    assert_eq!(n2.contains(IpAddr::V4(Ipv4Addr::new(2, 2, 2, 129))), true);
    assert_eq!(n2.contains(IpAddr::V4(Ipv4Addr::new(2, 2, 2, 5))), false);
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Nlri {
    Ip(IpNet),
}

impl Nlri {
    pub fn is_mp(self) -> bool {
        let Nlri::Ip(net) = self;
        !matches!(net.addr, IpAddr::V4(_))
    }
}

impl std::string::ToString for Nlri {
    fn to_string(&self) -> String {
        match self {
            Nlri::Ip(net) => format!("{}/{}", net.addr.to_string(), net.mask),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy, Eq, Hash)]
pub enum Family {
    Ipv4Uc,
    Ipv6Uc,

    Unknown(u32),
    Reserved,
}

impl From<Family> for u32 {
    fn from(family: Family) -> Self {
        match family {
            Family::Ipv4Uc => Family::IPV4_UC,
            Family::Ipv6Uc => Family::IPV6_UC,
            Family::Unknown(f) => f,
            Family::Reserved => 0,
        }
    }
}

impl From<u32> for Family {
    fn from(v: u32) -> Self {
        match v {
            Family::IPV4_UC => Family::Ipv4Uc,
            Family::IPV6_UC => Family::Ipv6Uc,
            0 => Family::Reserved,
            _ => Family::Unknown(v),
        }
    }
}

impl Family {
    const AFI_IP: u16 = 1;
    const AFI_IP6: u16 = 2;

    const SAFI_UNICAST: u8 = 1;

    const IPV4_UC: u32 = (Family::AFI_IP as u32) << 16 | Family::SAFI_UNICAST as u32;
    const IPV6_UC: u32 = (Family::AFI_IP6 as u32) << 16 | Family::SAFI_UNICAST as u32;

    pub fn afi(self) -> u16 {
        let family: u32 = From::from(self);
        (family >> 16) as u16
    }

    pub fn safi(self) -> u8 {
        let family: u32 = From::from(self);
        (family & 0xff) as u8
    }

    pub fn new(afi: u16, safi: u8) -> Family {
        Family::from((afi as u32) << 16 | safi as u32)
    }
}

pub struct OpenMessage {
    pub version: u8,
    pub as_number: u16,
    pub holdtime: u16,
    pub id: Ipv4Addr,
    pub params: Vec<OpenParam>,
    length: usize,
}

impl OpenMessage {
    const VERSION: u8 = 4;
    pub const HOLDTIME: u16 = 90;

    pub fn new(id: Ipv4Addr, caps: Vec<Capability>) -> OpenMessage {
        OpenMessage {
            version: OpenMessage::VERSION,
            as_number: AS_TRANS,
            holdtime: OpenMessage::HOLDTIME,
            id,
            params: caps
                .iter()
                .cloned()
                .map(OpenParam::CapabilityParam)
                .collect(),
            length: 0,
        }
    }

    pub fn get_as_number(&self) -> u32 {
        if self.as_number == AS_TRANS {
            for param in &self.params {
                if let OpenParam::CapabilityParam(Capability::FourOctetAsNumber { as_number }) =
                    param
                {
                    return *as_number;
                }
            }
        }
        self.as_number as u32
    }

    pub fn get_parameters(&self) -> Vec<OpenParam> {
        self.params.to_vec()
    }

    pub fn to_bytes(&self, c: &mut Cursor<Vec<u8>>) -> Result<usize, Error> {
        c.write_u8(self.version)?;
        c.write_u16::<NetworkEndian>(self.as_number)?;
        c.write_u16::<NetworkEndian>(self.holdtime)?;
        c.write_all(&self.id.octets())?;

        let pos_length = c.position();
        c.write_u8(0)?;

        let mut param_len = 0;
        for param in &self.params {
            param.to_bytes(c).map(|n| {
                param_len += n;
                n
            })?;
        }

        if param_len != 0 {
            let pos = c.position();
            c.set_position(pos_length);
            c.write_u8(param_len as u8)?;
            c.set_position(pos);
        }

        Ok(10 + param_len)
    }

    pub fn from_bytes(c: &mut Cursor<&[u8]>) -> Result<OpenMessage, Error> {
        let version = c.read_u8()?;
        let as_number = c.read_u16::<NetworkEndian>()?;
        let holdtime = c.read_u16::<NetworkEndian>()?;
        let id: Ipv4Addr = From::from(c.read_u32::<NetworkEndian>()?);

        let mut param_len = c.read_u8()?;
        let mut params: Vec<OpenParam> = Vec::new();
        while param_len > 0 {
            let pos = c.position();
            let mut param = OpenParam::from_bytes(c)?;
            params.append(&mut param);
            let used = c.position() - pos;
            if used > param_len as u64 {
                param_len = 0;
            } else {
                param_len -= used as u8;
            }
        }
        Ok(OpenMessage {
            version,
            as_number,
            holdtime,
            id,
            params,
            length: c.get_ref().len(),
        })
    }
}

pub enum NotificationCode {
    MessageHeaderConnectionNotSynchronized,
    MessageHeaderBadMessageLength,
    MessageHeaderBadMessageType,

    OpenMessageUnsupportedVersionNumber,
    OpenMessageBadPeerAs,
    OpenMessageBadBgpIdentifier,
    OpenMessageUnsupportedOptionalParameter,
    OpenMessageUnacceptableHoldTime,
    OpenMessageUnsupportedCapability,

    UpdateMessageMalformedAttributeList,
    UpdateMessageUnrecognizedWellKnownAttribute,
    UpdateMessageMissingWellKnownAttribute,
    UpdateMessageAttributeFlagsError,
    UpdateMessageAttributeLengthError,
    UpdateMessageInvalidOrigin,
    UpdateMessageDeprecatedRoutingLoop,
    UpdateMessageInvalidNextHop,
    UpdateMessageOptionalAttributeEroor,
    UpdateMessageInvalidNetworkField,
    UpdateMessageMalformedAsPath,

    HoldTimerExpired,

    FsmOpensentState,
    FsmOpenConfirm,
    FsmEstablished,

    MaximumNumberOfPrefixes,
    AdministrativeShutdown,
    PeerDeconfigured,
    AdministrativeReset,
    ConnectionRejected,
    OtherConfigurationChange,
    ConnectionCollistionResolution,
    OutOfResource,
}

impl From<NotificationCode> for u16 {
    fn from(code: NotificationCode) -> Self {
        match code {
            NotificationCode::MessageHeaderConnectionNotSynchronized => {
                NotificationCode::MESSAGE_HEADER_ERROR << 8
                    | NotificationCode::CONNECTION_NOT_SYNCHRONIZED
            }
            NotificationCode::MessageHeaderBadMessageLength => {
                NotificationCode::MESSAGE_HEADER_ERROR << 8 | NotificationCode::BAD_MESSAGE_LENGTH
            }
            NotificationCode::MessageHeaderBadMessageType => {
                NotificationCode::MESSAGE_HEADER_ERROR << 8 | NotificationCode::BAD_MESSAGE_TYPE
            }

            NotificationCode::OpenMessageUnsupportedVersionNumber => {
                NotificationCode::OPEN_MESSAGE_ERROR << 8
                    | NotificationCode::UNSUPPORTED_VERSION_NUMBER
            }
            NotificationCode::OpenMessageBadPeerAs => {
                NotificationCode::OPEN_MESSAGE_ERROR << 8 | NotificationCode::BAD_PEER_AS
            }
            NotificationCode::OpenMessageBadBgpIdentifier => {
                NotificationCode::OPEN_MESSAGE_ERROR << 8 | NotificationCode::BAD_BGP_IDENTIFIER
            }
            NotificationCode::OpenMessageUnsupportedOptionalParameter => {
                NotificationCode::OPEN_MESSAGE_ERROR << 8
                    | NotificationCode::UNSUPPORTED_OPTIONAL_PARAMETER
            }
            NotificationCode::OpenMessageUnacceptableHoldTime => {
                NotificationCode::OPEN_MESSAGE_ERROR << 8 | NotificationCode::UNACCEPTABLE_HOLD_TIME
            }
            NotificationCode::OpenMessageUnsupportedCapability => {
                NotificationCode::OPEN_MESSAGE_ERROR << 8 | NotificationCode::UNSUPPORTED_CAPABILITY
            }

            NotificationCode::UpdateMessageMalformedAttributeList => {
                NotificationCode::UPDATE_MESSAGE_ERROR << 8
                    | NotificationCode::MALFORMED_ATTRIBUTE_LIST
            }
            NotificationCode::UpdateMessageUnrecognizedWellKnownAttribute => {
                NotificationCode::UPDATE_MESSAGE_ERROR << 8
                    | NotificationCode::UNRECOGNIZED_WELL_KNOWN_ATTRIBUTE
            }
            NotificationCode::UpdateMessageMissingWellKnownAttribute => {
                NotificationCode::UPDATE_MESSAGE_ERROR << 8
                    | NotificationCode::MISSING_WELL_KNOWN_ATTRIBUTE
            }
            NotificationCode::UpdateMessageAttributeFlagsError => {
                NotificationCode::UPDATE_MESSAGE_ERROR << 8
                    | NotificationCode::ATTRIBUTE_FLAGS_ERROR
            }
            NotificationCode::UpdateMessageAttributeLengthError => {
                NotificationCode::UPDATE_MESSAGE_ERROR << 8
                    | NotificationCode::ATTRIBUTE_LENGTH_ERROR
            }
            NotificationCode::UpdateMessageInvalidOrigin => {
                NotificationCode::UPDATE_MESSAGE_ERROR << 8
                    | NotificationCode::INVALID_ORIGIN_ATTRIBUTE
            }
            NotificationCode::UpdateMessageDeprecatedRoutingLoop => {
                NotificationCode::UPDATE_MESSAGE_ERROR << 8
                    | NotificationCode::DEPRECATED_ROUTING_LOOP
            }
            NotificationCode::UpdateMessageInvalidNextHop => {
                NotificationCode::UPDATE_MESSAGE_ERROR << 8
                    | NotificationCode::INVALID_NEXT_HOP_ATTRIBUTE
            }
            NotificationCode::UpdateMessageOptionalAttributeEroor => {
                NotificationCode::UPDATE_MESSAGE_ERROR << 8
                    | NotificationCode::OPTIONAL_ATTRIBUTE_ERROR
            }
            NotificationCode::UpdateMessageInvalidNetworkField => {
                NotificationCode::UPDATE_MESSAGE_ERROR << 8
                    | NotificationCode::INVALID_NETWORK_FIELD
            }
            NotificationCode::UpdateMessageMalformedAsPath => {
                NotificationCode::UPDATE_MESSAGE_ERROR << 8 | NotificationCode::MALFORMED_AS_PATH
            }

            NotificationCode::HoldTimerExpired => NotificationCode::HOLD_TIMER_EXPIRED << 8 | 1,

            NotificationCode::FsmOpensentState => {
                NotificationCode::FSM_ERROR << 8
                    | NotificationCode::RECEIVE_UNEXPECTED_MESSAGE_IN_OPENSENT_STATE
            }
            NotificationCode::FsmOpenConfirm => {
                NotificationCode::FSM_ERROR << 8
                    | NotificationCode::RECEIVE_UNEXPECTED_MESSAGE_IN_OPENCONFIRM_STATE
            }
            NotificationCode::FsmEstablished => {
                NotificationCode::FSM_ERROR << 8
                    | NotificationCode::RECEIVE_UNEXPECTED_MESSAGE_IN_ESTABLISHED_STATE
            }

            NotificationCode::MaximumNumberOfPrefixes => {
                NotificationCode::CEASE << 8 | NotificationCode::MAXIMUM_NUMBER_OF_PREFIXES_REACHED
            }
            NotificationCode::AdministrativeShutdown => {
                NotificationCode::CEASE << 8 | NotificationCode::ADMINISTRATIVE_SHUTDOWN
            }
            NotificationCode::PeerDeconfigured => {
                NotificationCode::CEASE << 8 | NotificationCode::PEER_DECONFIGURED
            }
            NotificationCode::AdministrativeReset => {
                NotificationCode::CEASE << 8 | NotificationCode::ADMINISTRATIVE_RESET
            }
            NotificationCode::ConnectionRejected => {
                NotificationCode::CEASE << 8 | NotificationCode::CONNECTION_REJECTED
            }
            NotificationCode::OtherConfigurationChange => {
                NotificationCode::CEASE << 8 | NotificationCode::OTHER_CONFIGURATION_CHANGE
            }
            NotificationCode::ConnectionCollistionResolution => {
                NotificationCode::CEASE << 8 | NotificationCode::CONNECTION_COLLISION_RESOLUTION
            }
            NotificationCode::OutOfResource => {
                NotificationCode::CEASE << 8 | NotificationCode::OUT_OF_RESOURCES
            }
        }
    }
}

impl NotificationCode {
    const MESSAGE_HEADER_ERROR: u16 = 1;
    const OPEN_MESSAGE_ERROR: u16 = 2;
    const UPDATE_MESSAGE_ERROR: u16 = 3;
    const HOLD_TIMER_EXPIRED: u16 = 4;
    const FSM_ERROR: u16 = 5;
    const CEASE: u16 = 6;
    // const ROUTE_REFRESH_MESSAGE_ERROR: u16 = 7;

    // Message Header Error subcodes
    const CONNECTION_NOT_SYNCHRONIZED: u16 = 1;
    const BAD_MESSAGE_LENGTH: u16 = 2;
    const BAD_MESSAGE_TYPE: u16 = 3;

    // OPEN Message Error subcodes
    const UNSUPPORTED_VERSION_NUMBER: u16 = 1;
    const BAD_PEER_AS: u16 = 2;
    const BAD_BGP_IDENTIFIER: u16 = 3;
    const UNSUPPORTED_OPTIONAL_PARAMETER: u16 = 4;
    //const DEPRECATED_AUTHENTICATION_FAILURE
    const UNACCEPTABLE_HOLD_TIME: u16 = 6;
    const UNSUPPORTED_CAPABILITY: u16 = 7;

    // Update Message Error subcodes
    const MALFORMED_ATTRIBUTE_LIST: u16 = 1;
    const UNRECOGNIZED_WELL_KNOWN_ATTRIBUTE: u16 = 2;
    const MISSING_WELL_KNOWN_ATTRIBUTE: u16 = 3;
    const ATTRIBUTE_FLAGS_ERROR: u16 = 4;
    const ATTRIBUTE_LENGTH_ERROR: u16 = 5;
    const INVALID_ORIGIN_ATTRIBUTE: u16 = 6;
    const DEPRECATED_ROUTING_LOOP: u16 = 7;
    const INVALID_NEXT_HOP_ATTRIBUTE: u16 = 8;
    const OPTIONAL_ATTRIBUTE_ERROR: u16 = 9;
    const INVALID_NETWORK_FIELD: u16 = 10;
    const MALFORMED_AS_PATH: u16 = 11;

    // fsm Error subcodes
    const RECEIVE_UNEXPECTED_MESSAGE_IN_OPENSENT_STATE: u16 = 1;
    const RECEIVE_UNEXPECTED_MESSAGE_IN_OPENCONFIRM_STATE: u16 = 2;
    const RECEIVE_UNEXPECTED_MESSAGE_IN_ESTABLISHED_STATE: u16 = 3;

    // cease Error subcodes
    const MAXIMUM_NUMBER_OF_PREFIXES_REACHED: u16 = 1;
    const ADMINISTRATIVE_SHUTDOWN: u16 = 2;
    const PEER_DECONFIGURED: u16 = 3;
    const ADMINISTRATIVE_RESET: u16 = 4;
    const CONNECTION_REJECTED: u16 = 5;
    const OTHER_CONFIGURATION_CHANGE: u16 = 6;
    const CONNECTION_COLLISION_RESOLUTION: u16 = 7;
    const OUT_OF_RESOURCES: u16 = 8;
}

pub struct NotificationMessage {
    pub code: u8,
    pub sub_code: u8,
    length: usize,
}

impl NotificationMessage {
    pub fn new(code: NotificationCode) -> NotificationMessage {
        let v: u16 = From::from(code);
        NotificationMessage {
            code: (v >> 8) as u8,
            sub_code: (v & 0xff) as u8,
            length: 2,
        }
    }

    pub fn to_bytes(&self, c: &mut Cursor<Vec<u8>>) -> Result<usize, Error> {
        c.write_u8(self.code)?;
        c.write_u8(self.sub_code)?;

        Ok(2)
    }

    pub fn from_bytes(c: &mut Cursor<&[u8]>) -> Result<NotificationMessage, Error> {
        let code = c.read_u8()?;
        let sub_code = c.read_u8()?;
        let length = c.get_ref().len();
        for _ in 0..length - 2 {
            c.read_u8()?;
        }

        Ok(NotificationMessage {
            code,
            sub_code,
            length,
        })
    }
}

pub struct RouteRefreshMessage {
    pub family: Family,
    pub demarcation: u8,
}

impl RouteRefreshMessage {
    pub fn from_bytes(c: &mut Cursor<&[u8]>) -> Result<RouteRefreshMessage, Error> {
        let afi = c.read_u16::<NetworkEndian>()?;
        let demarcation = c.read_u8()?;
        let safi = c.read_u8()?;

        Ok(RouteRefreshMessage {
            family: Family::from((afi as u32) << 16 | safi as u32),
            demarcation,
        })
    }
}

pub struct ParseParam {
    pub local_as: u32,
}

pub enum Message {
    Open(OpenMessage),
    Update(UpdateMessage),
    Notification(NotificationMessage),
    Keepalive,
    RouteRefresh(RouteRefreshMessage),
    Unknown { length: usize, code: u8 },
}

impl Message {
    const HEADER_LENGTH: u16 = 19;

    const OPEN: u8 = 1;
    const UPDATE: u8 = 2;
    const NOTIFICATION: u8 = 3;
    const KEEPALIVE: u8 = 4;
    const ROUTE_REFRESH: u8 = 5;

    fn to_u8(&self) -> u8 {
        match self {
            Message::Open(_) => Message::OPEN,
            Message::Update(_) => Message::UPDATE,
            Message::Notification(_) => Message::NOTIFICATION,
            Message::Keepalive => Message::KEEPALIVE,
            Message::RouteRefresh(_) => Message::ROUTE_REFRESH,
            Message::Unknown { code, .. } => *code,
        }
    }

    pub fn length(&self) -> usize {
        let mut len = Message::HEADER_LENGTH as usize;
        match self {
            Message::Open(m) => len += m.length,
            Message::Update(m) => len += m.length,
            Message::Notification(m) => len += m.length,
            Message::RouteRefresh(_) => len += 4,
            Message::Unknown { length, .. } => len += length,
            _ => {}
        }
        len
    }

    pub fn from_bytes(param: &ParseParam, buf: &[u8]) -> Result<Message, Error> {
        let buflen = buf.len();
        let mut c = Cursor::new(buf);

        if buflen < Message::HEADER_LENGTH as usize {
            return Err(Error::InvalidFormat);
        }
        c.set_position(16);
        let length = c.read_u16::<NetworkEndian>()?;
        if buflen < length as usize {
            return Err(Error::InvalidFormat);
        }

        let code = c.read_u8()?;
        let mut c = Cursor::new(&buf[Message::HEADER_LENGTH as usize..length as usize]);
        match code {
            Message::OPEN => {
                let b = OpenMessage::from_bytes(&mut c)?;
                Ok(Message::Open(b))
            }
            Message::UPDATE => {
                let b = UpdateMessage::from_bytes(param, &mut c)?;
                Ok(Message::Update(b))
            }
            Message::NOTIFICATION => {
                let b = NotificationMessage::from_bytes(&mut c)?;
                Ok(Message::Notification(b))
            }
            Message::KEEPALIVE => Ok(Message::Keepalive),
            Message::ROUTE_REFRESH => {
                let b = RouteRefreshMessage::from_bytes(&mut c)?;
                Ok(Message::RouteRefresh(b))
            }
            _ => {
                let body_length = length - Message::HEADER_LENGTH;
                for _ in 0..body_length {
                    c.read_u8()?;
                }
                Ok(Message::Unknown {
                    length: body_length as usize,
                    code,
                })
            }
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        let buf: Vec<u8> = Vec::new();
        let mut c = Cursor::new(buf);
        c.write_all(&[0xff; 16])?;
        // length: might be update later.
        let pos_length = c.position();
        c.write_u16::<NetworkEndian>(Message::HEADER_LENGTH)?;
        // type
        c.write_all(&[self.to_u8()])?;

        let mut body_length = 0;
        match self {
            Message::Open(b) => {
                if let Ok(n) = b.to_bytes(&mut c) {
                    body_length += n;
                }
            }
            // Message::Update(b) => match b.to_bytes(&mut c) {
            //     Ok(n) => body_length += n,
            //     Err(_) => {}
            // },
            Message::Notification(b) => {
                if let Ok(n) = b.to_bytes(&mut c) {
                    body_length += n;
                }
            }
            _ => {}
        }

        if body_length != 0 {
            let pos = c.position();
            c.set_position(pos_length);
            c.write_u16::<NetworkEndian>(Message::HEADER_LENGTH + body_length as u16)?;
            c.set_position(pos);
        }
        Ok(c.into_inner())
    }

    fn header_bytes(c: &mut Cursor<Vec<u8>>, t: u8, body_length: u16) -> Result<usize, Error> {
        let pos = c.position();
        c.write_all(&[0xff; 16])?;
        c.write_u16::<NetworkEndian>(Message::HEADER_LENGTH + body_length)?;
        c.write_u8(t)?;
        Ok((c.position() - pos) as usize)
    }
}

#[derive(PartialOrd, PartialEq, Clone, Copy)]
pub enum State {
    Idle,
    Connect,
    Active,
    OpenSent,
    OpenConfirm,
    Established,
}

#[derive(Clone)]
pub struct Segment {
    pub segment_type: u8,
    pub number: Vec<u32>,
}

impl Segment {
    pub const TYPE_SET: u8 = 1;
    pub const TYPE_SEQ: u8 = 2;
    pub const TYPE_CONFED_SET: u8 = 3;
    pub const TYPE_CONFED_SEQ: u8 = 4;

    pub fn as_len(&self) -> usize {
        match self.segment_type {
            Segment::TYPE_SET => 1,
            Segment::TYPE_SEQ => self.number.len(),
            _ => 0,
        }
    }

    pub fn new(t: u8, number: &[u32]) -> Segment {
        Segment {
            segment_type: t,
            number: number.iter().map(|x| x.to_owned()).collect(),
        }
    }
}

#[derive(Clone)]
pub enum Attribute {
    Origin {
        origin: u8,
    },
    AsPath {
        segments: Vec<Segment>,
    },
    Nexthop {
        nexthop: IpAddr,
    },
    MultiExitDesc {
        descriptor: u32,
    },
    LocalPref {
        preference: u32,
    },
    AtomicAggregate,
    Aggregator {
        four_byte: bool,
        number: u32,
        address: IpAddr,
    },
    Community {
        communities: Vec<u32>,
    },
    OriginatorId {
        address: IpAddr,
    },
    ClusterList {
        addresses: Vec<IpAddr>,
    },
    MpReach {
        family: Family,
        // TODO: link-local
        nexthop: IpAddr,
        nlri: Vec<Nlri>,
    },
    MpUnreach {
        family: Family,
        nlri: Vec<Nlri>,
    },
    // ExtendedCommunity,
    // As4Path,
    // As4Aggregator,

    // PmsiTunnel,
    // TunnelEncap,
    // TraficEngineering,
    // IpV6ExtendedCommunity,
    NotSupported {
        attr_flag: u8,
        attr_type: u8,
        attr_len: u16,
        buf: Vec<u8>,
    },
}

impl Attribute {
    const FLAG_EXTENDED: u8 = 1 << 4;
    // const FLAG_PARTIAL: u8 = 1 << 5;
    const FLAG_TRANSITIVE: u8 = 1 << 6;
    const FLAG_OPTIONAL: u8 = 1 << 7;

    pub const ORIGIN: u8 = 1;
    pub const AS_PATH: u8 = 2;
    pub const NEXTHOP: u8 = 3;
    pub const MULTI_EXIT_DESC: u8 = 4;
    pub const LOCAL_PREF: u8 = 5;
    pub const ATOMIC_AGGREGATE: u8 = 6;
    pub const AGGREGATOR: u8 = 7;
    pub const COMMUNITY: u8 = 8;
    pub const ORIGINATOR_ID: u8 = 9;
    pub const CLUSTER_LIST: u8 = 10;
    pub const MP_REACH: u8 = 14;
    pub const MP_UNREACH: u8 = 15;

    pub const DEFAULT_LOCAL_PREF: u32 = 100;

    fn length_error() -> Error {
        Error::from(std::io::Error::new(
            std::io::ErrorKind::Other,
            "invalid attribute length",
        ))
    }

    pub fn from_bytes(c: &mut Cursor<&[u8]>) -> Result<Attribute, Error> {
        // flag
        let attr_flag = c.read_u8()?;

        // type
        let attr_type = c.read_u8()?;

        // attribute len
        let mut attr_len = 0;
        if attr_flag & Attribute::FLAG_EXTENDED != 0 {
            attr_len += c.read_u16::<NetworkEndian>()?
        } else {
            attr_len += c.read_u8()? as u16;
        }

        if ((attr_flag ^ Attribute::flag(attr_type))
            & (Attribute::FLAG_OPTIONAL | Attribute::FLAG_TRANSITIVE))
            > 0
        {
            for _i in 0..attr_len {
                c.read_u8()?;
            }
            return Err(Error::InvalidFormat);
        }

        match attr_type {
            Attribute::ORIGIN => {
                let origin = c.read_u8()?;
                Ok(Attribute::Origin { origin })
            }
            Attribute::AS_PATH => {
                let mut segments: Vec<Segment> = Vec::new();
                while attr_len > 0 {
                    let code = c.read_u8()?;
                    let num = c.read_u8()?;
                    let mut numbers = Vec::new();
                    for _ in 0..num {
                        numbers.push(c.read_u32::<NetworkEndian>()?);
                    }
                    segments.push(Segment {
                        segment_type: code,
                        number: numbers,
                    });
                    let used = (2 + num * 4) as u16;
                    if attr_len < used {
                        attr_len = 0;
                    } else {
                        attr_len -= used;
                    }
                }
                Ok(Attribute::AsPath { segments })
            }
            Attribute::NEXTHOP => {
                if attr_len == 4 {
                    let mut buf = [0; 4];
                    c.read_exact(&mut buf)?;
                    return Ok(Attribute::Nexthop {
                        nexthop: IpAddr::from(buf),
                    });
                } else if attr_len == 16 {
                    let mut buf = [0; 16];
                    c.read_exact(&mut buf)?;
                    return Ok(Attribute::Nexthop {
                        nexthop: IpAddr::from(buf),
                    });
                }
                Err(Attribute::length_error())
            }
            Attribute::MULTI_EXIT_DESC => {
                let descriptor = c.read_u32::<NetworkEndian>()?;
                Ok(Attribute::MultiExitDesc { descriptor })
            }
            Attribute::LOCAL_PREF => {
                let preference = c.read_u32::<NetworkEndian>()?;
                Ok(Attribute::LocalPref { preference })
            }
            Attribute::ATOMIC_AGGREGATE => Ok(Attribute::AtomicAggregate {}),
            Attribute::AGGREGATOR => {
                if attr_len == 6 {
                    let number = c.read_u16::<NetworkEndian>()?;
                    let mut buf = [0; 4];
                    c.read_exact(&mut buf)?;
                    return Ok(Attribute::Aggregator {
                        four_byte: false,
                        number: number as u32,
                        address: IpAddr::from(buf),
                    });
                } else if attr_len == 8 {
                    let number = c.read_u32::<NetworkEndian>()?;
                    let mut buf = [0; 4];
                    c.read_exact(&mut buf)?;
                    return Ok(Attribute::Aggregator {
                        four_byte: true,
                        number,
                        address: IpAddr::from(buf),
                    });
                }
                Err(Attribute::length_error())
            }
            Attribute::COMMUNITY => {
                if attr_len % 4 == 0 {
                    let mut communities: Vec<u32> = Vec::new();
                    while attr_len > 0 {
                        communities.push(c.read_u32::<NetworkEndian>()?);
                        attr_len -= 4;
                    }
                    return Ok(Attribute::Community { communities });
                }
                Err(Attribute::length_error())
            }
            Attribute::ORIGINATOR_ID => {
                if attr_len == 4 {
                    let mut buf = [0; 4];
                    c.read_exact(&mut buf)?;
                    return Ok(Attribute::OriginatorId {
                        address: IpAddr::from(buf),
                    });
                }
                Err(Attribute::length_error())
            }
            Attribute::MP_REACH => {
                let afi = c.read_u16::<NetworkEndian>()?;
                let safi = c.read_u8()?;
                let nexthop_len = c.read_u8()?;
                let nexthop = match nexthop_len {
                    4 => {
                        let mut buf = [0; 4];
                        c.read_exact(&mut buf)?;
                        IpAddr::from(buf)
                    }
                    16 => {
                        let mut buf = [0; 16];
                        c.read_exact(&mut buf)?;
                        IpAddr::from(buf)
                    }
                    32 => {
                        // TODO
                        let mut buf = [0; 16];
                        c.read_exact(&mut buf)?;
                        c.read_exact(&mut buf)?;
                        IpAddr::from(buf)
                    }
                    _ => return Err(Attribute::length_error()),
                };
                c.read_u8()?;

                let nlri_len = attr_len - (2 + 1 + 2 + nexthop_len as u16 + 1);
                let nlri_end = c.position() + nlri_len as u64;
                let mut mp_routes: Vec<Nlri> = Vec::new();
                while c.position() < nlri_end {
                    let net = IpNet::from_bytes(c, true)?;
                    mp_routes.push(Nlri::Ip(net));
                }
                Ok(Attribute::MpReach {
                    family: Family::new(afi, safi),
                    nlri: mp_routes,
                    nexthop,
                })
            }
            Attribute::MP_UNREACH => {
                let afi = c.read_u16::<NetworkEndian>()?;
                let safi = c.read_u8()?;

                let mut withdrawn: Vec<Nlri> = Vec::new();
                let nlri_len = attr_len - 3;
                let nlri_end = c.position() + nlri_len as u64;
                while c.position() < nlri_end {
                    let net = IpNet::from_bytes(c, true)?;
                    withdrawn.push(Nlri::Ip(net))
                }

                Ok(Attribute::MpUnreach {
                    family: Family::new(afi, safi),
                    nlri: withdrawn,
                })
            }
            _ => {
                let mut buf: Vec<u8> = Vec::new();
                for _ in 0..attr_len {
                    buf.push(c.read_u8()?);
                }
                Ok(Attribute::NotSupported {
                    attr_flag,
                    attr_type,
                    attr_len,
                    buf,
                })
            }
        }
    }

    pub fn to_bytes(&self, c: &mut Cursor<Vec<u8>>) -> Result<usize, Error> {
        let pos = c.position();

        let t = self.attr();
        let mut flag = Attribute::flag(t);
        match self {
            Attribute::AsPath { .. }
            | Attribute::Community { .. }
            | Attribute::MpReach { .. }
            | Attribute::MpUnreach { .. } => flag |= Attribute::FLAG_EXTENDED,
            Attribute::NotSupported { attr_flag, .. } => flag = *attr_flag,
            _ => {}
        }

        c.write_u8(flag)?;
        c.write_u8(t)?;

        match self {
            Attribute::Origin { origin } => {
                c.write_u8(1)?;
                c.write_u8(*origin)?;
            }
            Attribute::AsPath { segments } => {
                let mut len = 0;
                for segment in segments {
                    len += 2 + segment.number.len() * 4;
                }
                c.write_u16::<NetworkEndian>(len as u16)?;
                for segment in segments {
                    c.write_u8(segment.segment_type)?;
                    c.write_u8(segment.number.len() as u8)?;
                    for n in &segment.number {
                        c.write_u32::<NetworkEndian>(*n)?;
                    }
                }
            }
            Attribute::Nexthop { nexthop } => match nexthop {
                IpAddr::V4(addr) => {
                    c.write_u8(4)?;
                    c.write_u32::<NetworkEndian>(u32::from(*addr))?;
                }
                IpAddr::V6(addr) => {
                    c.write_u8(16)?;
                    for i in &addr.octets() {
                        c.write_u8(*i)?;
                    }
                }
            },
            Attribute::MultiExitDesc { descriptor } => {
                c.write_u8(4)?;
                c.write_u32::<NetworkEndian>(*descriptor)?;
            }
            Attribute::LocalPref { preference } => {
                c.write_u8(4)?;
                c.write_u32::<NetworkEndian>(*preference)?;
            }
            Attribute::AtomicAggregate => {
                c.write_u8(0)?;
            }
            Attribute::Aggregator {
                four_byte,
                number,
                address,
            } => {
                if *four_byte {
                    c.write_u8(8)?;
                    c.write_u32::<NetworkEndian>(*number)?;
                } else {
                    c.write_u8(6)?;
                    c.write_u16::<NetworkEndian>(*number as u16)?;
                }
                if let IpAddr::V4(addr) = address {
                    c.write_u32::<NetworkEndian>(u32::from(*addr))?;
                }
            }
            Attribute::Community { communities } => {
                c.write_u16::<NetworkEndian>(communities.len() as u16 * 4)?;
                for i in communities {
                    c.write_u32::<NetworkEndian>(*i)?;
                }
            }
            Attribute::OriginatorId { address } => {
                c.write_u8(4)?;
                if let IpAddr::V4(addr) = address {
                    c.write_u32::<NetworkEndian>(u32::from(*addr))?;
                }
            }
            Attribute::ClusterList { .. } => {}
            Attribute::MpReach {
                family,
                nexthop,
                nlri,
            } => {
                let mut l = match nexthop {
                    IpAddr::V4(_) => 4,
                    IpAddr::V6(_) => 16,
                };
                l += 2 + 1 + 1 + 1;
                for r in nlri {
                    let Nlri::Ip(ip) = r;
                    l += ip.size();
                }

                c.write_u16::<NetworkEndian>(l as u16)?;
                c.write_u16::<NetworkEndian>(family.afi())?;
                c.write_u8(family.safi())?;
                match nexthop {
                    IpAddr::V4(addr) => {
                        c.write_u8(4)?;
                        c.write_u32::<NetworkEndian>(u32::from(*addr))?
                    }
                    IpAddr::V6(addr) => {
                        c.write_u8(16)?;
                        for i in &addr.octets() {
                            c.write_u8(*i)?;
                        }
                    }
                }
                c.write_u8(0)?;
                for r in nlri {
                    let Nlri::Ip(ip) = r;
                    ip.to_bytes(c)?;
                }
            }
            Attribute::MpUnreach { family, nlri } => {
                let mut nlri_len = 0;
                for r in nlri {
                    let Nlri::Ip(ip) = r;
                    nlri_len += ip.size();
                }

                c.write_u16::<NetworkEndian>(3 + nlri_len as u16)?;
                c.write_u16::<NetworkEndian>(family.afi())?;
                c.write_u8(family.safi())?;
                for r in nlri {
                    let Nlri::Ip(ip) = r;
                    ip.to_bytes(c)?;
                }
            }
            // ExtendedCommunity,
            // As4Path,
            // As4Aggregator,
            // PmsiTunnel,
            // TunnelEncap,
            // TraficEngineering,
            // IpV6ExtendedCommunity,
            Attribute::NotSupported {
                attr_flag,
                attr_len,
                buf,
                ..
            } => {
                if (*attr_flag & Attribute::FLAG_EXTENDED) != 0 {
                    c.write_u16::<NetworkEndian>(*attr_len)?;
                } else {
                    c.write_u8(*attr_len as u8)?;
                }
                for i in buf {
                    c.write_u8(*i)?;
                }
            }
        }
        Ok((c.position() - pos) as usize)
    }

    pub fn flag(t: u8) -> u8 {
        match t {
            Attribute::ORIGIN => Attribute::FLAG_TRANSITIVE,
            Attribute::AS_PATH => Attribute::FLAG_TRANSITIVE,
            Attribute::NEXTHOP => Attribute::FLAG_TRANSITIVE,
            Attribute::MULTI_EXIT_DESC => Attribute::FLAG_OPTIONAL,
            Attribute::LOCAL_PREF => Attribute::FLAG_TRANSITIVE,
            Attribute::ATOMIC_AGGREGATE => Attribute::FLAG_TRANSITIVE,
            Attribute::AGGREGATOR => Attribute::FLAG_TRANSITIVE | Attribute::FLAG_OPTIONAL,
            Attribute::COMMUNITY => Attribute::FLAG_TRANSITIVE | Attribute::FLAG_OPTIONAL,
            Attribute::ORIGINATOR_ID => Attribute::FLAG_OPTIONAL,
            Attribute::CLUSTER_LIST => Attribute::FLAG_OPTIONAL,
            Attribute::MP_REACH => Attribute::FLAG_OPTIONAL,
            Attribute::MP_UNREACH => Attribute::FLAG_OPTIONAL,
            // ExtendedCommunity,
            // As4Path,
            // As4Aggregator,
            // PmsiTunnel,
            // TunnelEncap,
            // TraficEngineering,
            // IpV6ExtendedCommunity,
            _ => Attribute::FLAG_OPTIONAL,
        }
    }

    pub fn attr(&self) -> u8 {
        match self {
            Attribute::Origin { .. } => Attribute::ORIGIN,
            Attribute::AsPath { .. } => Attribute::AS_PATH,
            Attribute::Nexthop { .. } => Attribute::NEXTHOP,
            Attribute::MultiExitDesc { .. } => Attribute::MULTI_EXIT_DESC,
            Attribute::LocalPref { .. } => Attribute::LOCAL_PREF,
            Attribute::AtomicAggregate { .. } => Attribute::ATOMIC_AGGREGATE,
            Attribute::Aggregator { .. } => Attribute::AGGREGATOR,
            Attribute::Community { .. } => Attribute::COMMUNITY,
            Attribute::OriginatorId { .. } => Attribute::ORIGINATOR_ID,
            Attribute::ClusterList { .. } => Attribute::CLUSTER_LIST,
            Attribute::MpReach { .. } => Attribute::MP_REACH,
            Attribute::MpUnreach { .. } => Attribute::MP_UNREACH,
            Attribute::NotSupported { attr_type, .. } => *attr_type,
        }
    }

    pub fn is_transitive(&self) -> bool {
        let mut flag = Attribute::flag(self.attr());
        if let Attribute::NotSupported { attr_flag, .. } = self {
            flag = *attr_flag;
        }
        flag & Attribute::FLAG_TRANSITIVE != 0
    }
}

#[test]
fn path_attribute_origin() {
    let buf = Vec::new();
    let mut c = Cursor::new(buf);
    let _ = Attribute::Origin { origin: 3 }.to_bytes(&mut c).unwrap();
    let c: &[u8] = &c.get_ref();
    match Attribute::from_bytes(&mut Cursor::new(c)).unwrap() {
        Attribute::Origin { origin } => assert_eq!(origin, 3),
        _ => assert!(false),
    }
}

#[test]
fn path_attribute_nexthop_v4() {
    let addr = IpAddr::V4(Ipv4Addr::new(12, 0, 0, 1));
    let buf = Vec::new();
    let mut c = Cursor::new(buf);
    let _ = Attribute::Nexthop { nexthop: addr }
        .to_bytes(&mut c)
        .unwrap();
    let c: &[u8] = &c.get_ref();
    match Attribute::from_bytes(&mut Cursor::new(c)).unwrap() {
        Attribute::Nexthop { nexthop } => assert_eq!(nexthop, addr),
        _ => assert!(false),
    }
}

#[test]
fn path_attribute_nexthop_v6() {
    use std::net::Ipv6Addr;
    let addr = IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0xc00a, 0x2ff));
    let buf = Vec::new();
    let mut c = Cursor::new(buf);
    let _ = Attribute::Nexthop { nexthop: addr }
        .to_bytes(&mut c)
        .unwrap();
    let c: &[u8] = &c.get_ref();
    match Attribute::from_bytes(&mut Cursor::new(c)).unwrap() {
        Attribute::Nexthop { nexthop } => assert_eq!(nexthop, addr),
        _ => assert!(false),
    }
}

#[test]
fn path_attribute_as_path() {
    let buf = Vec::new();
    let segments = vec![
        Segment {
            segment_type: 2,
            number: vec![3, 4],
        },
        Segment {
            segment_type: 1,
            number: vec![5, 6, 7],
        },
    ];
    let mut c = Cursor::new(buf);
    let _ = Attribute::AsPath { segments }.to_bytes(&mut c).unwrap();
    let c: &[u8] = &c.get_ref();
    match Attribute::from_bytes(&mut Cursor::new(c)).unwrap() {
        Attribute::AsPath { segments: segs } => {
            assert_eq!(segs[0].segment_type, 2);
            assert_eq!(segs[0].number[0], 3);
            assert_eq!(segs[0].number[1], 4);

            assert_eq!(segs[1].segment_type, 1);
            assert_eq!(segs[1].number.len(), 3);
            assert_eq!(segs[1].number[0], 5);
            assert_eq!(segs[1].number[1], 6);
        }
        _ => assert!(false),
    }
}

pub struct UpdateMessage {
    pub attrs: Vec<Attribute>,
    pub routes: Vec<Nlri>,
    pub nexthop: IpAddr,
    pub mp_routes: Option<(Family, Vec<Nlri>, IpAddr)>,
    pub withdrawns: Vec<(Family, Nlri)>,
    length: usize,
}

impl UpdateMessage {
    const INVALID_NEXTHOP: IpAddr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));

    pub fn new(
        routes: Vec<Nlri>,
        mp_routes: Option<(Family, Vec<Nlri>, IpAddr)>,
        withdrawns: Vec<(Family, Nlri)>,
        attrs: Vec<Attribute>,
    ) -> UpdateMessage {
        UpdateMessage {
            routes,
            mp_routes,
            withdrawns,
            attrs,
            nexthop: UpdateMessage::INVALID_NEXTHOP,
            length: 0,
        }
    }

    pub fn from_bytes(param: &ParseParam, c: &mut Cursor<&[u8]>) -> Result<UpdateMessage, Error> {
        let withdrawn_len = c.read_u16::<NetworkEndian>()?;
        let mut withdrawns: Vec<(Family, Nlri)> = Vec::new();
        let mut ip_nexthop = UpdateMessage::INVALID_NEXTHOP;

        let pos = c.position();
        while c.position() - pos < withdrawn_len as u64 {
            let net = IpNet::from_bytes(c, false)?;
            withdrawns.push((Family::Ipv4Uc, Nlri::Ip(net)));
        }

        let attr_len = c.read_u16::<NetworkEndian>()?;
        let mut attrs: Vec<Attribute> = Vec::new();

        let mut handle_as_withdrawns = false;
        let mut seen = HashSet::new();
        let attr_end = c.position() + attr_len as u64;
        let mut mp_routes: Option<(Family, Vec<Nlri>, IpAddr)> = None;
        while c.position() < attr_end {
            let attr = Attribute::from_bytes(c);
            match attr {
                Ok(a) => {
                    if !seen.insert(a.attr()) {
                        // ignore duplicated attribute
                        continue;
                    }
                    match &a {
                        Attribute::Nexthop { nexthop } => ip_nexthop = *nexthop,
                        Attribute::AsPath { segments } => {
                            for seg in segments {
                                for n in &seg.number {
                                    if *n == param.local_as {
                                        handle_as_withdrawns = true;
                                    }
                                }
                            }
                            attrs.push(a);
                        }
                        Attribute::MpReach {
                            family,
                            nlri,
                            nexthop,
                        } => {
                            mp_routes = Some((*family, nlri.to_vec(), *nexthop));
                        }
                        Attribute::MpUnreach { family, nlri } => {
                            withdrawns
                                .append(&mut nlri.iter().cloned().map(|n| (*family, n)).collect());
                        }
                        _ => attrs.push(a),
                    }
                }
                Err(e) => {
                    // io:Error means that we can't parse any more
                    if let Error::Std(_) = e {
                        return Err(e);
                    }
                    handle_as_withdrawns = true;
                }
            }
        }

        let mut routes: Vec<Nlri> = Vec::new();

        while c.get_ref().len() > c.position() as usize {
            let net = IpNet::from_bytes(c, false)?;
            routes.push(Nlri::Ip(net));
        }

        if (!routes.is_empty() || mp_routes.is_some())
            && (handle_as_withdrawns
                || !seen.contains(&Attribute::ORIGIN)
                || !seen.contains(&Attribute::AS_PATH)
                || (!routes.is_empty() && !seen.contains(&Attribute::NEXTHOP)))
        {
            withdrawns.append(&mut routes.iter().map(|n| (Family::Ipv4Uc, *n)).collect());
            if let Some(a) = mp_routes {
                withdrawns.append(&mut a.1.iter().map(|n| (a.0, *n)).collect());
                mp_routes = None;
            }
        }

        Ok(UpdateMessage {
            attrs,
            routes,
            withdrawns,
            nexthop: ip_nexthop,
            mp_routes,
            length: c.get_ref().len(),
        })
    }

    // FIXME
    pub fn bytes(
        routes: Vec<Nlri>,
        withdrawns: Vec<Nlri>,
        attrs: Vec<&Attribute>,
    ) -> Result<Vec<u8>, Error> {
        let buf: Vec<u8> = Vec::new();
        let mut c = Cursor::new(buf);

        let start_pos = Message::HEADER_LENGTH as u64;

        c.set_position(start_pos + 2);
        let mut withdrawn_len = 0;
        let mut mp_withdrawns: Vec<Nlri> = Vec::new();
        for withdrawn in withdrawns {
            if withdrawn.is_mp() {
                mp_withdrawns.push(withdrawn);
                continue;
            }
            match withdrawn {
                Nlri::Ip(ip) => {
                    withdrawn_len += ip.to_bytes(&mut c)?;
                }
            }
        }
        let attr_pos = c.position();
        c.set_position(start_pos);
        c.write_u16::<NetworkEndian>(withdrawn_len as u16)?;
        c.set_position(attr_pos + 2);

        let mut attr_len = 0;
        for attr in attrs {
            attr_len += attr.to_bytes(&mut c)?;
        }
        if !mp_withdrawns.is_empty() {
            attr_len += Attribute::MpUnreach {
                family: Family::Ipv6Uc,
                nlri: mp_withdrawns,
            }
            .to_bytes(&mut c)?;
        }

        let route_pos = c.position();
        c.set_position(attr_pos);
        c.write_u16::<NetworkEndian>(attr_len as u16)?;
        c.set_position(route_pos);
        for route in routes {
            match route {
                Nlri::Ip(ip) => {
                    ip.to_bytes(&mut c)?;
                }
            }
        }
        let body_length = c.position() - start_pos;
        c.set_position(0);
        Message::header_bytes(&mut c, Message::UPDATE, body_length as u16)?;

        Ok(c.into_inner())
    }
}

#[derive(Debug, Clone)]
pub enum OpenParam {
    CapabilityParam(Capability),
    UnknownParam,
}

impl OpenParam {
    pub fn from_bytes(c: &mut Cursor<&[u8]>) -> Result<Vec<OpenParam>, Error> {
        let code = c.read_u8()?;
        let mut len = c.read_u8()?;

        match code {
            2 => {
                let mut r = Vec::new();
                while len > 0 {
                    let pos = c.position();
                    match Capability::from_bytes(c) {
                        Ok(cap) => r.push(OpenParam::CapabilityParam(cap)),
                        Err(err) => return Err(err),
                    }
                    let used = c.position() - pos;
                    if used > len as u64 {
                        len = 0;
                    } else {
                        len -= used as u8;
                    }
                }
                Ok(r)
            }
            _ => {
                for _ in 0..len {
                    c.read_u8()?;
                }
                Ok(vec![OpenParam::UnknownParam])
            }
        }
    }

    pub fn to_bytes(&self, c: &mut Cursor<Vec<u8>>) -> Result<usize, Error> {
        match self {
            OpenParam::CapabilityParam(cap) => {
                c.write_u8(2)?;
                let length_pos = c.position();
                c.write_u8(0)?;
                cap.to_bytes(c).and_then(|n| {
                    let pos = c.position();
                    c.set_position(length_pos);
                    c.write_u8(n as u8)?;
                    c.set_position(pos);
                    Ok(2 + n as usize)
                })
            }
            OpenParam::UnknownParam => Ok(0),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Capability {
    MultiProtocol {
        family: Family,
    },
    RouteRefresh,
    CarryingLabelInfo,
    ExtendedNexthop {
        values: Vec<(Family, Family)>,
    },
    GracefulRestart {
        flags: u8,
        time: u16,
        values: Vec<(Family, u8)>,
    },
    FourOctetAsNumber {
        as_number: u32,
    },
    AddPath {
        values: Vec<(Family, u8)>,
    },
    EnhanshedRouteRefresh,
    LongLivedGracefulRestart {
        values: Vec<(Family, u8, u32)>,
    },
    RouteRefreshCisco,

    Unknown {
        code: u8,
        values: Vec<u8>,
    },
}

impl Capability {
    const MULTI_PROTOCOL: u8 = 1;
    const ROUTE_REFRESH: u8 = 2;
    const CARRYING_LABEL_INFO: u8 = 4;
    const EXTENDED_NEXTHOP: u8 = 5;
    const GRACEFUL_RESTART: u8 = 64;
    const FOUR_OCTET_AS_NUMBER: u8 = 65;
    const ADD_PATH: u8 = 69;
    const ENHANCED_ROUTE_REFRESH: u8 = 70;
    const LONG_LIVED_GRACEFUL_RESTART: u8 = 71;
    const ROUTE_REFRESH_CISCO: u8 = 128;

    pub fn from_bytes(c: &mut Cursor<&[u8]>) -> Result<Capability, Error> {
        let code = c.read_u8()?;
        let mut len = c.read_u8()?;

        match code {
            Capability::MULTI_PROTOCOL => {
                let family = c.read_u32::<NetworkEndian>()?;
                Ok(Capability::MultiProtocol {
                    family: Family::from(family),
                })
            }
            Capability::ROUTE_REFRESH => Ok(Capability::RouteRefresh),
            Capability::CARRYING_LABEL_INFO => Ok(Capability::CarryingLabelInfo),
            Capability::EXTENDED_NEXTHOP => {
                let mut v = Vec::new();
                while len > 0 {
                    v.push((
                        From::from(c.read_u32::<NetworkEndian>()?),
                        From::from(
                            (c.read_u16::<NetworkEndian>()? as u32) << 16
                                | Family::SAFI_UNICAST as u32,
                        ),
                    ));
                    len -= 6;
                }
                Ok(Capability::ExtendedNexthop { values: v })
            }
            Capability::GRACEFUL_RESTART => {
                let mut v = Vec::new();
                let restart = c.read_u16::<NetworkEndian>()?;
                let flags = (restart >> 12) as u8;
                let time = restart & 0xfff;
                len -= 2;
                while len > 0 {
                    let afi = c.read_u16::<NetworkEndian>()? as u32;
                    let safi = c.read_u8()? as u32;
                    v.push((From::from(afi << 16 | safi), c.read_u8()?));
                    len -= 4;
                }
                Ok(Capability::GracefulRestart {
                    flags,
                    time,
                    values: v,
                })
            }
            Capability::FOUR_OCTET_AS_NUMBER => {
                let as_number = c.read_u32::<NetworkEndian>()?;
                Ok(Capability::FourOctetAsNumber { as_number })
            }
            Capability::ADD_PATH => {
                let mut v = Vec::new();
                while len > 0 {
                    let afi = c.read_u16::<NetworkEndian>()? as u32;
                    let safi = c.read_u8()? as u32;
                    v.push((From::from(afi << 16 | safi), c.read_u8()?));
                    len -= 4;
                }
                Ok(Capability::AddPath { values: v })
            }
            Capability::ENHANCED_ROUTE_REFRESH => Ok(Capability::EnhanshedRouteRefresh),
            Capability::LONG_LIVED_GRACEFUL_RESTART => {
                let mut v = Vec::new();
                while len > 0 {
                    let afi = c.read_u16::<NetworkEndian>()? as u32;
                    let safi = c.read_u8()? as u32;
                    let flags = c.read_u8()?;
                    let time = (c.read_u8()? as u32) << 16
                        | (c.read_u8()? as u32) << 8
                        | c.read_u8()? as u32;
                    v.push((From::from(afi << 16 | safi), flags, time));
                    len -= 7;
                }
                Ok(Capability::LongLivedGracefulRestart { values: v })
            }
            Capability::ROUTE_REFRESH_CISCO => Ok(Capability::RouteRefreshCisco),
            _ => {
                let mut v = Vec::new();
                for _ in 0..len {
                    v.push(c.read_u8()?);
                }
                Ok(Capability::Unknown { code, values: v })
            }
        }
    }

    pub fn to_bytes(&self, c: &mut Cursor<Vec<u8>>) -> Result<usize, Error> {
        let pos = c.position();
        match self {
            Capability::MultiProtocol { family } => {
                c.write_u8(Capability::MULTI_PROTOCOL)?;
                c.write_u8(4)?;
                c.write_u16::<NetworkEndian>(family.afi())?;
                c.write_u8(0)?;
                c.write_u8(family.safi())?;
            }
            Capability::RouteRefresh => {
                c.write_u8(Capability::ROUTE_REFRESH)?;
                c.write_u8(0)?;
            }
            Capability::FourOctetAsNumber { as_number } => {
                c.write_u8(Capability::FOUR_OCTET_AS_NUMBER)?;
                c.write_u8(4)?;
                c.write_u32::<NetworkEndian>(*as_number)?;
            }
            _ => {}
        }
        Ok((c.position() - pos) as usize)
    }
}

pub enum WellKnownCommunity {
    GracefulShutdown,
    AcceptOwn,
    LlgrStale,
    NoLlgr,
    Blackhole,
    NoExport,
    NoAdvertise,
    NoExportSubconfed,
    NoPeer,
}

impl FromStr for WellKnownCommunity {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "graceful-shutdown" => Ok(WellKnownCommunity::GracefulShutdown),
            "accept-own" => Ok(WellKnownCommunity::AcceptOwn),
            "llgr-stale" => Ok(WellKnownCommunity::LlgrStale),
            "no-llgr" => Ok(WellKnownCommunity::NoLlgr),
            "blackhole" => Ok(WellKnownCommunity::Blackhole),
            "no-export" => Ok(WellKnownCommunity::NoExport),
            "no-advertise" => Ok(WellKnownCommunity::NoAdvertise),
            "no-export-subconfed" => Ok(WellKnownCommunity::NoExportSubconfed),
            "no-peer" => Ok(WellKnownCommunity::NoPeer),
            _ => Err(Error::InvalidFormat),
        }
    }
}

impl From<WellKnownCommunity> for u32 {
    fn from(c: WellKnownCommunity) -> Self {
        match c {
            WellKnownCommunity::GracefulShutdown => 0xffff_0000,
            WellKnownCommunity::AcceptOwn => 0xffff_0001,
            WellKnownCommunity::LlgrStale => 0xffff_0006,
            WellKnownCommunity::NoLlgr => 0xffff_0007,
            WellKnownCommunity::Blackhole => 0xffff_029a,
            WellKnownCommunity::NoExport => 0xffff_ff01,
            WellKnownCommunity::NoAdvertise => 0xffff_ff02,
            WellKnownCommunity::NoExportSubconfed => 0xffff_ff03,
            WellKnownCommunity::NoPeer => 0xffff_ff04,
        }
    }
}
