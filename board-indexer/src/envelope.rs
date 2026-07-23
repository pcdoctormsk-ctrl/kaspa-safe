use secp256k1::{schnorr, Message, Secp256k1, XOnlyPublicKey};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MAGIC: &[u8; 4] = b"KBRD";
pub const VERSION_V1: u8 = 1;
pub const VERSION_V2: u8 = 2;
pub const IDENTITY_ANON_DERIVED: u8 = 0;
pub const IDENTITY_TRIP: u8 = 1;
pub const BOARD_MAX: usize = 16;
pub const SUBJECT_MAX: usize = 64;
pub const BODY_MAX: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardPost {
    pub is_op: bool,
    pub board: String,
    pub parent_txid: Option<[u8; 32]>,
    pub subject: String,
    pub ephemeral_pk: [u8; 32],
    pub recovery_nonce: Option<[u8; 16]>,
    pub image_sha256: Option<[u8; 32]>,
    pub body: String,
    pub sig_start: usize,
    pub sig: [u8; 64],
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BoardParseError {
    #[error("bad KBRD magic")]
    BadMagic,
    #[error("unsupported KBRD version")]
    BadVersion,
    #[error("invalid KBRD identity mode")]
    BadIdentityMode,
    #[error("truncated KBRD envelope")]
    TooShort,
    #[error("{0} exceeds the protocol limit")]
    FieldTooLong(&'static str),
    #[error("{0} is not UTF-8")]
    BadUtf8(&'static str),
    #[error("invalid body length")]
    BadBodyLen,
    #[error("invalid BIP340 signature")]
    BadSig,
    #[error("bytes follow the KBRD signature")]
    TrailingBytes,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], BoardParseError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(BoardParseError::TooShort)?;
        if end > self.bytes.len() {
            return Err(BoardParseError::TooShort);
        }
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, BoardParseError> {
        Ok(self.take(1)?[0])
    }

    fn arr<const N: usize>(&mut self) -> Result<[u8; N], BoardParseError> {
        let mut value = [0; N];
        value.copy_from_slice(self.take(N)?);
        Ok(value)
    }
}

struct RawFields<'a> {
    is_op: bool,
    board: &'a [u8],
    parent_txid: Option<[u8; 32]>,
    subject: &'a [u8],
    ephemeral_pk: [u8; 32],
    recovery_nonce: Option<[u8; 16]>,
    image_sha256: Option<[u8; 32]>,
    body: &'a [u8],
    sig_start: usize,
    sig: [u8; 64],
}

fn parse_raw(payload: &[u8]) -> Result<RawFields<'_>, BoardParseError> {
    let mut cursor = Cursor {
        bytes: payload,
        offset: 0,
    };
    if cursor.take(MAGIC.len())? != MAGIC {
        return Err(BoardParseError::BadMagic);
    }
    let version = cursor.u8()?;
    if version != VERSION_V1 && version != VERSION_V2 {
        return Err(BoardParseError::BadVersion);
    }
    let flags = cursor.u8()?;
    let is_op = flags & 0b01 != 0;
    let has_image = flags & 0b10 != 0;
    let recovery_nonce = if version == VERSION_V2 {
        let identity_mode = cursor.u8()?;
        let nonce = cursor.arr::<16>()?;
        match identity_mode {
            IDENTITY_ANON_DERIVED => Some(nonce),
            IDENTITY_TRIP if nonce.iter().all(|byte| *byte == 0) => None,
            _ => return Err(BoardParseError::BadIdentityMode),
        }
    } else {
        None
    };
    let board_len = cursor.u8()? as usize;
    if board_len == 0 || board_len > BOARD_MAX {
        return Err(BoardParseError::FieldTooLong("board"));
    }
    let board = cursor.take(board_len)?;
    let parent_txid = if is_op {
        None
    } else {
        Some(cursor.arr::<32>()?)
    };
    let subject_len = cursor.u8()? as usize;
    if subject_len > SUBJECT_MAX {
        return Err(BoardParseError::FieldTooLong("subject"));
    }
    let subject = cursor.take(subject_len)?;
    let ephemeral_pk = cursor.arr::<32>()?;
    let image_sha256 = if has_image {
        Some(cursor.arr::<32>()?)
    } else {
        None
    };
    let body_len = u16::from_le_bytes([cursor.u8()?, cursor.u8()?]) as usize;
    if body_len == 0 || body_len > BODY_MAX {
        return Err(BoardParseError::BadBodyLen);
    }
    let body = cursor.take(body_len)?;
    let sig_start = cursor.offset;
    let sig = cursor.arr::<64>()?;
    if cursor.offset != payload.len() {
        return Err(BoardParseError::TrailingBytes);
    }
    Ok(RawFields {
        is_op,
        board,
        parent_txid,
        subject,
        ephemeral_pk,
        recovery_nonce,
        image_sha256,
        body,
        sig_start,
        sig,
    })
}

fn decode(raw: RawFields<'_>) -> Result<BoardPost, BoardParseError> {
    Ok(BoardPost {
        is_op: raw.is_op,
        board: std::str::from_utf8(raw.board)
            .map_err(|_| BoardParseError::BadUtf8("board"))?
            .to_owned(),
        parent_txid: raw.parent_txid,
        subject: std::str::from_utf8(raw.subject)
            .map_err(|_| BoardParseError::BadUtf8("subject"))?
            .to_owned(),
        ephemeral_pk: raw.ephemeral_pk,
        recovery_nonce: raw.recovery_nonce,
        image_sha256: raw.image_sha256,
        body: std::str::from_utf8(raw.body)
            .map_err(|_| BoardParseError::BadUtf8("body"))?
            .to_owned(),
        sig_start: raw.sig_start,
        sig: raw.sig,
    })
}

pub fn parse_fields(payload: &[u8]) -> Result<BoardPost, BoardParseError> {
    decode(parse_raw(payload)?)
}

pub fn parse_and_verify(payload: &[u8]) -> Result<BoardPost, BoardParseError> {
    let raw = parse_raw(payload)?;
    let public_key =
        XOnlyPublicKey::from_slice(&raw.ephemeral_pk).map_err(|_| BoardParseError::BadSig)?;
    let digest: [u8; 32] = Sha256::digest(&payload[..raw.sig_start]).into();
    let message = Message::from_digest(digest);
    let signature =
        schnorr::Signature::from_slice(&raw.sig).map_err(|_| BoardParseError::BadSig)?;
    Secp256k1::verification_only()
        .verify_schnorr(&signature, &message, &public_key)
        .map_err(|_| BoardParseError::BadSig)?;
    decode(raw)
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use secp256k1::{Keypair, Secp256k1};

    #[allow(clippy::too_many_arguments)]
    pub fn signed_payload(
        is_op: bool,
        version: u8,
        board: &str,
        parent: Option<[u8; 32]>,
        subject: &str,
        body: &str,
        secret: u8,
        image: Option<[u8; 32]>,
    ) -> Vec<u8> {
        let secp = Secp256k1::new();
        let keypair = Keypair::from_seckey_slice(&secp, &[secret; 32]).unwrap();
        let (public_key, _) = XOnlyPublicKey::from_keypair(&keypair);
        let mut payload = Vec::new();
        payload.extend_from_slice(MAGIC);
        payload.push(version);
        payload.push((is_op as u8) | if image.is_some() { 0b10 } else { 0 });
        if version == VERSION_V2 {
            payload.push(IDENTITY_ANON_DERIVED);
            payload.extend_from_slice(&[9; 16]);
        }
        payload.push(board.len() as u8);
        payload.extend_from_slice(board.as_bytes());
        if !is_op {
            payload.extend_from_slice(&parent.unwrap());
        }
        payload.push(subject.len() as u8);
        payload.extend_from_slice(subject.as_bytes());
        payload.extend_from_slice(&public_key.serialize());
        if let Some(hash) = image {
            payload.extend_from_slice(&hash);
        }
        payload.extend_from_slice(&(body.len() as u16).to_le_bytes());
        payload.extend_from_slice(body.as_bytes());
        let digest: [u8; 32] = Sha256::digest(&payload).into();
        let signature = secp.sign_schnorr_no_aux_rand(&Message::from_digest(digest), &keypair);
        payload.extend_from_slice(signature.as_ref());
        payload
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::signed_payload;
    use super::*;

    #[test]
    fn verifies_v1_and_v2() {
        for version in [VERSION_V1, VERSION_V2] {
            let payload = signed_payload(true, version, "g", None, "subject", "body", 7, None);
            let post = parse_and_verify(&payload).unwrap();
            assert_eq!(post.board, "g");
            assert_eq!(
                post.recovery_nonce,
                (version == VERSION_V2).then_some([9; 16])
            );
        }
    }

    #[test]
    fn rejects_tampering_and_trailing_bytes() {
        let payload = signed_payload(true, VERSION_V2, "g", None, "", "body", 8, None);
        let mut tampered = payload.clone();
        let body_byte = tampered.len() - 65;
        tampered[body_byte] ^= 1;
        assert_eq!(
            parse_and_verify(&tampered).unwrap_err(),
            BoardParseError::BadSig
        );

        let mut trailing = payload;
        trailing.push(0);
        assert_eq!(
            parse_fields(&trailing).unwrap_err(),
            BoardParseError::TrailingBytes
        );
    }

    #[test]
    fn bounds_checks_untrusted_input() {
        assert_eq!(
            parse_fields(b"KBRD").unwrap_err(),
            BoardParseError::TooShort
        );
        let long_board = "x".repeat(BOARD_MAX + 1);
        let payload = signed_payload(true, VERSION_V1, &long_board, None, "", "x", 9, None);
        assert_eq!(
            parse_fields(&payload).unwrap_err(),
            BoardParseError::FieldTooLong("board")
        );
    }
}
