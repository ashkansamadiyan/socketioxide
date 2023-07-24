//! ## Encoder for http payloads
//!
//! There is 3 different encoders:
//! * engine.io v4 encoder
//! * engine.io v3 encoder:
//!    * string encoder (used when there is no binary packet or when the client does not support binary)
//!    * binary encoder (used when there is binary packets and the client supports binary)
//!

use tokio::sync::{mpsc::Receiver, MutexGuard};
use tracing::debug;

use crate::{errors::Error, packet::Packet};

/// Encode multiple packets into a string payload according to the
/// [engine.io v4 protocol](https://socket.io/fr/docs/v4/engine-io-protocol/#http-long-polling-1)
#[cfg(feature = "v4")]
pub async fn v4_encoder(mut rx: MutexGuard<'_, Receiver<Packet>>) -> Result<Vec<u8>, Error> {
    use crate::payload::PACKET_SEPARATOR_V4;

    let mut data: String = String::new();

    // Send all packets in the buffer
    while let Ok(packet) = rx.try_recv() {
        debug!("sending packet: {:?}", packet);
        let packet: String = packet.try_into()?;

        if !data.is_empty() {
            data.push(std::char::from_u32(PACKET_SEPARATOR_V4 as u32).unwrap());
        }
        data.push_str(&packet);
    }

    // If there is no packet in the buffer, wait for the next packet
    if data.is_empty() {
        let packet = rx.recv().await.ok_or(Error::Aborted)?;
        debug!("sending packet: {:?}", packet);
        let packet: String = packet.try_into()?;
        data.push_str(&packet);
    }
    Ok(data.into())
}

/// Encode one packet into a *binary* payload according to the
/// [engine.io v3 protocol](https://github.com/socketio/engine.io-protocol/tree/v3#payload)
#[cfg(feature = "v3")]
pub fn v3_bin_packet_encoder(packet: Packet, data: &mut Vec<u8>) -> Result<(), Error> {
    use bytes::BufMut;
    match packet {
        Packet::BinaryV3(bin) => {
            data.push(0x1);

            let len = bin.len() + 1;
            let leading_zero_bytes = len.leading_zeros() / 8;
            data.put_slice(&len.to_be_bytes()[leading_zero_bytes as usize..]);
            data.push(0xff); // separator
            data.push(0x04); // message packet type
            data.extend_from_slice(&bin); // raw data
        }
        packet => {
            let packet: String = packet.try_into()?;
            data.push(0x0); // 0 = string

            let len = packet.len();
            let leading_zero_bytes = len.leading_zeros() / 8;
            data.put_slice(&len.to_be_bytes()[leading_zero_bytes as usize..]);

            data.push(0xff); // separator
            data.extend_from_slice(packet.as_bytes()); // packet
        }
    };
    Ok(())
}

/// Encode one packet into a *string* payload according to the
/// [engine.io v3 protocol](https://github.com/socketio/engine.io-protocol/tree/v3#payload)
#[cfg(feature = "v3")]
pub fn v3_string_packet_encoder(packet: Packet, data: &mut Vec<u8>) -> Result<(), Error> {
    use crate::payload::PACKET_SEPARATOR_V3;
    let packet: String = packet.try_into()?;
    let packet = format!(
        "{}{}{}",
        packet.chars().count(),
        PACKET_SEPARATOR_V3 as char,
        packet
    );
    data.extend_from_slice(packet.as_bytes());
    Ok(())
}

/// Encode multiple packet packet into a *string* payload if there is no binary packet or into a *binary* payload if there is binary packets
/// according to the [engine.io v4 protocol](https://socket.io/fr/docs/v4/engine-io-protocol/#http-long-polling-1)
#[cfg(feature = "v3")]
pub async fn v3_binary_encoder(mut rx: MutexGuard<'_, Receiver<Packet>>) -> Result<Vec<u8>, Error> {
    let mut data: Vec<u8> = Vec::new();
    let mut packet_buffer: Vec<Packet> = Vec::new();

    // buffer all packets to find if there is binary packets
    let mut has_binary = false;
    while let Ok(packet) = rx.try_recv() {
        if packet.is_binary() {
            has_binary = true;
        }
        debug!("sending packet: {:?}", packet);
        packet_buffer.push(packet);
    }

    if has_binary {
        for packet in packet_buffer {
            v3_bin_packet_encoder(packet, &mut data)?
        }
    } else {
        for packet in packet_buffer {
            v3_string_packet_encoder(packet, &mut data)?;
        }
    }

    // If there is no packet in the buffer, wait for the next packet
    if data.is_empty() {
        let packet = rx.recv().await.ok_or(Error::Aborted)?;
        debug!("sending packet: {:?}", packet);
        match packet {
            Packet::BinaryV3(_) | Packet::Binary(_) => {
                v3_bin_packet_encoder(packet, &mut data)?;
            }
            packet => {
                v3_string_packet_encoder(packet, &mut data)?;
            }
        };
    }

    Ok(data)
}

/// Encode multiple packet packet into a *string* payload according to the
/// [engine.io v3 protocol](https://github.com/socketio/engine.io-protocol/tree/v3#payload)
#[cfg(feature = "v3")]
pub async fn v3_string_encoder(mut rx: MutexGuard<'_, Receiver<Packet>>) -> Result<Vec<u8>, Error> {
    let mut data: Vec<u8> = Vec::new();

    while let Ok(packet) = rx.try_recv() {
        v3_string_packet_encoder(packet, &mut data)?;
    }

    // If there is no packet in the buffer, wait for the next packet
    if data.is_empty() {
        let packet = rx.recv().await.ok_or(Error::Aborted)?;
        v3_string_packet_encoder(packet, &mut data)?;
    }

    Ok(data)
}
