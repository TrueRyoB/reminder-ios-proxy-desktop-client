//! SRP-6a client implementation matching Apple's idmsa variant.
//!
//! This mirrors `cocagne/pysrp` (`srp` PyPI package) as used by pyicloud with
//! `rfc5054_enable()` and `no_username_in_x()` enabled:
//!   - k = H(N || PAD(g))                       (RFC5054 multiplier)
//!   - u = H(PAD(A) || PAD(B))                   (PAD = left-zero-pad to len(N))
//!   - x = H(salt || H(":" + derived_password))   (username blanked, but the
//!     ":" separator is NOT dropped -- this is the easy-to-miss part of
//!     Apple's "no username in x" variant)
//!   - M1 = H(HNxorg(N,g) || H(username) || salt || A || B || K)
//!   - H_AMK = H(A || M1 || K)
//!
//! `derived_password` is itself PBKDF2-HMAC-SHA256(digest, salt, iterations,
//! key_length) where `digest` is SHA256(password) for the "s2k" protocol, or
//! the lowercase-hex ASCII encoding of that SHA256 digest for "s2k_fo".
//!
//! Every formula here was verified against the real `pysrp` reference
//! implementation with fixed inputs -- see `tests` below.

use num_bigint_dig::BigUint;
use num_traits::Zero;
use sha2::{Digest, Sha256};

/// RFC5054 2048-bit group.
const N_HEX: &str = "\
AC6BDB41324A9A9BF166DE5E1389582FAF72B6651987EE07FC3192943DB56050A37329CBB4\
A099ED8193E0757767A13DD52312AB4B03310DCD7F48A9DA04FD50E8083969EDB767B0CF60\
95179A163AB3661A05FBD5FAAAE82918A9962F0B93B855F97993EC975EEAA80D740ADBF4FF\
747359D041D5C33EA71D281E446B14773BCA97B43A23FB801676BD207A436C6481F1D2B907\
8717461A5B9D32E688F87748544523B524B0D57D5EA77A2775D2ECFA032CFBDBF52FB37861\
60279004E57AE6AF874E7303CE53299CCC041C7BC308D82A5698F3A8D0C38271AE35F8E9DB\
FBB694B5C803D89F7AE435DE236D525F54759B65E372FCD68EF20FA7111F9E4AFF73";
const G_DEC: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SrpProtocol {
    S2k,
    S2kFo,
}

#[derive(Debug, thiserror::Error)]
pub enum SrpError {
    #[error("SRP-6a safety check failed: B % N == 0")]
    ServerPublicIsZero,
    #[error("SRP-6a safety check failed: u == 0")]
    ScramblingParamIsZero,
}

/// Left-pad `data` with zero bytes so its length equals `width`.
/// Panics if `data` is already longer than `width` (should never happen for
/// values bound by modulus N, mirroring pysrp's `bytes(width - len(data))`
/// which raises on a negative count).
fn pad_left(data: &[u8], width: usize) -> Vec<u8> {
    assert!(data.len() <= width, "value longer than pad width");
    let mut out = vec![0u8; width - data.len()];
    out.extend_from_slice(data);
    out
}

fn sha256(parts: &[&[u8]]) -> Vec<u8> {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().to_vec()
}

/// PBKDF2-HMAC-SHA256(digest, salt, iterations, key_length), where `digest`
/// is derived from the raw password per the Apple SRP "protocol" variant.
pub fn derive_password(
    password: &str,
    salt: &[u8],
    iterations: u32,
    key_length: usize,
    protocol: SrpProtocol,
) -> Vec<u8> {
    let password_hash = sha256(&[password.as_bytes()]);
    let digest: Vec<u8> = match protocol {
        SrpProtocol::S2k => password_hash,
        SrpProtocol::S2kFo => hex::encode(password_hash).into_bytes(),
    };
    let mut out = vec![0u8; key_length];
    pbkdf2::pbkdf2_hmac::<Sha256>(&digest, salt, iterations, &mut out);
    out
}

/// A fresh random 256-byte (2048-bit) client secret ephemeral, matching
/// pysrp's `get_random_of_length(256)` (top bit forced set is not required
/// here since modpow reduces mod N regardless; pysrp itself doesn't rely on
/// that top-bit-set property for security, only for byte-length stability).
pub fn random_secret_256() -> Vec<u8> {
    use rand::RngCore;
    let mut bytes = vec![0u8; 256];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
}

fn n_g() -> (BigUint, BigUint) {
    let n = BigUint::parse_bytes(N_HEX.as_bytes(), 16).expect("valid N hex");
    let g = BigUint::from(G_DEC);
    (n, g)
}

fn k_value(n: &BigUint, g: &BigUint) -> BigUint {
    let n_bytes = n.to_bytes_be();
    let width = n_bytes.len();
    let g_bytes = pad_left(&g.to_bytes_be(), width);
    BigUint::from_bytes_be(&sha256(&[&n_bytes, &g_bytes]))
}

fn u_value(n_byte_len: usize, a_pub: &BigUint, b_pub: &BigUint) -> BigUint {
    let a_bytes = pad_left(&a_pub.to_bytes_be(), n_byte_len);
    let b_bytes = pad_left(&b_pub.to_bytes_be(), n_byte_len);
    BigUint::from_bytes_be(&sha256(&[&a_bytes, &b_bytes]))
}

/// x = H(salt || H(":" + derived_password)) -- username is blanked (Apple's
/// no_username_in_x variant) but the ":" separator from the standard
/// `H(username + ":" + password)` formula is NOT removed.
fn gen_x(salt: &[u8], derived_password: &[u8]) -> BigUint {
    let inner = sha256(&[b":", derived_password]);
    BigUint::from_bytes_be(&sha256(&[salt, &inner]))
}

/// H(N) XOR H(PAD(g)) where g is left-padded to len(N) bytes.
fn hn_xor_g(n: &BigUint, g: &BigUint) -> Vec<u8> {
    let n_bytes = n.to_bytes_be();
    let g_bytes = pad_left(&g.to_bytes_be(), n_bytes.len());
    let hn = sha256(&[&n_bytes]);
    let hg = sha256(&[&g_bytes]);
    hn.iter().zip(hg.iter()).map(|(a, b)| a ^ b).collect()
}

fn calculate_m1(
    n: &BigUint,
    g: &BigUint,
    username: &str,
    salt: &[u8],
    a_pub: &BigUint,
    b_pub: &BigUint,
    session_key: &[u8],
) -> Vec<u8> {
    let hnxorg = hn_xor_g(n, g);
    let h_username = sha256(&[username.as_bytes()]);
    sha256(&[
        &hnxorg,
        &h_username,
        salt,
        &a_pub.to_bytes_be(),
        &b_pub.to_bytes_be(),
        session_key,
    ])
}

fn calculate_h_amk(a_pub: &BigUint, m1: &[u8], session_key: &[u8]) -> Vec<u8> {
    sha256(&[&a_pub.to_bytes_be(), m1, session_key])
}

/// One SRP-6a client-side handshake (mirrors pysrp's `User`).
pub struct SrpClient {
    n: BigUint,
    g: BigUint,
    a_secret: BigUint,
    pub a_pub: BigUint,
    username: String,
}

pub struct ChallengeResult {
    pub m1: Vec<u8>,
    pub h_amk: Vec<u8>,
    /// Reserved: not needed for the idmsa web-login flow itself, but kept
    /// for debugging and in case a later flow (e.g. 2FA) needs it.
    #[allow(dead_code)]
    pub session_key: Vec<u8>,
}

impl SrpClient {
    /// `a_secret` should normally be 256 random bytes (2048-bit); callers
    /// pass it in explicitly so tests can pin it to a fixed value.
    pub fn new(username: &str, a_secret_bytes: &[u8]) -> Self {
        let (n, g) = n_g();
        let a_secret = BigUint::from_bytes_be(a_secret_bytes);
        let a_pub = g.modpow(&a_secret, &n);
        Self {
            n,
            g,
            a_secret,
            a_pub,
            username: username.to_string(),
        }
    }

    pub fn public_ephemeral(&self) -> Vec<u8> {
        self.a_pub.to_bytes_be()
    }

    /// Process the server's (salt, B) challenge and produce (M1, H_AMK, K).
    pub fn process_challenge(
        &self,
        salt: &[u8],
        b_pub_bytes: &[u8],
        derived_password: &[u8],
    ) -> Result<ChallengeResult, SrpError> {
        let b_pub = BigUint::from_bytes_be(b_pub_bytes);
        if (&b_pub % &self.n).is_zero() {
            return Err(SrpError::ServerPublicIsZero);
        }

        let n_byte_len = self.n.to_bytes_be().len();
        let u = u_value(n_byte_len, &self.a_pub, &b_pub);
        if u.is_zero() {
            return Err(SrpError::ScramblingParamIsZero);
        }

        let x = gen_x(salt, derived_password);
        let k = k_value(&self.n, &self.g);
        let v = self.g.modpow(&x, &self.n);

        let kv_mod = (&k * &v) % &self.n;
        let base = if b_pub >= kv_mod {
            (&b_pub - &kv_mod) % &self.n
        } else {
            (&self.n - (&kv_mod - &b_pub)) % &self.n
        };
        let exponent = &self.a_secret + &u * &x;
        let s = base.modpow(&exponent, &self.n);

        let session_key = sha256(&[&s.to_bytes_be()]);
        let m1 = calculate_m1(
            &self.n,
            &self.g,
            &self.username,
            salt,
            &self.a_pub,
            &b_pub,
            &session_key,
        );
        let h_amk = calculate_h_amk(&self.a_pub, &m1, &session_key);

        Ok(ChallengeResult {
            m1,
            h_amk,
            session_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_to_bytes(s: &str) -> Vec<u8> {
        hex::decode(s).expect("valid hex")
    }

    /// Vectors generated by running the real `pysrp` library (the same one
    /// pyicloud depends on) with `rfc5054_enable()` + `no_username_in_x()`
    /// and fixed salt/a/b, verified there via a full client+Verifier
    /// round-trip before being copied here. See gen_test_vectors.py.
    struct Vector {
        protocol: SrpProtocol,
        salt: &'static str,
        a_secret: &'static str,
        b_pub: &'static str,
        derived_password: &'static str,
        a_pub: &'static str,
        m1: &'static str,
        h_amk: &'static str,
    }

    const VECTORS: [Vector; 2] = [
        Vector {
            protocol: SrpProtocol::S2k,
            salt: "0102030405060708090a0b0c0d0e0f10",
            a_secret: "11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
            b_pub: "569d99ab769ab72ffc6d0b7457d08faf5feb6e0d2f552a59eafc53873f5d51a796c208781cc21f99938ce7eed04c0635c7663275e45d2ea2037f5984f43aecf80bafdb934dda8316583f9a48cc4008a250d73005963e71129501aeace0db5a69ca9c905d2f67f3fb176f3ca815aa2a1296eac047ef3fa9640cb97b3577d91985f34c214dfcdcd578bd9813891ada0888f33095b1d89263604617598e1c328127f682f481b5559916e9c11d2c1f76b598798fc5c9b1fea72b8430eb49c0ad7e55b72a6a204ad114bd248c29f970d3aa9794325b78706dcc201c334aff5558b69495c2b9cc00a5c86036c220f8bfb4848cff89b701f43a7e4102d8a3edb67d664e",
            derived_password: "4f84e41c94f96f6d7a5e1531d010f258ebb6803a50fec744d8d59ed4bcb03e64",
            a_pub: "641b7ee890330aba07a89acb27f62f81beafad3a8261f33d38a2247ff13f4d99fb2555498cde617538b117451e4e6ff3641fccd6b4e2962bafc38b1e24c411f10e5a9e577b5ae967df0a0bdda7372d7b2586e10b8af1f353db1f17e39706a99b13e604b7da614550432caec53068117ca91889eef511133ced547873e93f99c6862919a45cd92e8f7fbc5fd5cb16a30e8c7bf6708c58444d22feefb5520577daf10d7f85e511eecae01935d535e0b575c8f59fa793e0ca815e1158c3744b0068c19eac778cb33f8414da86021c492989682f135a897496c7312c54abde93cf10593d67ddd999bb013e10ff2a8b52d43385e7bd22f9e0b1c721b1baa332f43830",
            m1: "670bbe1862a68889b973896f21d7899d1c13631d863dfd333da6432ef0231b23",
            h_amk: "8879ee72fdcf69f40e4addd4c6715514b73df66b84c497e0809e3136e1c5fa6e",
        },
        Vector {
            protocol: SrpProtocol::S2kFo,
            salt: "0102030405060708090a0b0c0d0e0f10",
            a_secret: "11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
            b_pub: "0b3e8f69ada16859d851c27f6eb43ca3f80d06734eed9c09e3930c1d47d71fbbacc49d79a0c8a3ad0a7c1863a29549759626ef2728546fbcee13f278190e41824a26afedee7e81bf26bbba78fbc00fdbf9bc44ab196da8d023a8f5e6fe3467d44e105ecff41ef521c852902bdbc09b33a5ba359085e07c23a0665c15ca8391ee8215babd859f3a3e6c35e0d94bddd6552f6724d8a3096a204a42fe693f90468216d798cb24f6e52faabdea21654c3650f3a6c9b54f518c276d2418dd981700a4bf781d62f6c9d843fde9f2923bbeea0e7918aeb652728271266d07309d7d171ddb1c1a792e4fb7d04e1116465d79c50a1063585e31e4c1e9d9ad25e3ee0c23f0",
            derived_password: "11bd3409dd3235b8bb7b850387390346b097bee2c1e71697a6d9b5f377e89658",
            a_pub: "641b7ee890330aba07a89acb27f62f81beafad3a8261f33d38a2247ff13f4d99fb2555498cde617538b117451e4e6ff3641fccd6b4e2962bafc38b1e24c411f10e5a9e577b5ae967df0a0bdda7372d7b2586e10b8af1f353db1f17e39706a99b13e604b7da614550432caec53068117ca91889eef511133ced547873e93f99c6862919a45cd92e8f7fbc5fd5cb16a30e8c7bf6708c58444d22feefb5520577daf10d7f85e511eecae01935d535e0b575c8f59fa793e0ca815e1158c3744b0068c19eac778cb33f8414da86021c492989682f135a897496c7312c54abde93cf10593d67ddd999bb013e10ff2a8b52d43385e7bd22f9e0b1c721b1baa332f43830",
            m1: "6251e736d436b81d89b8c1da7dc393c291a30cadc40e5daebbe9903a3009e312",
            h_amk: "d60f12f309dc043ddab24b100179423e29dd1653f2e2b852a051f4ffa5d78699",
        },
    ];

    #[test]
    fn matches_pysrp_reference_vectors() {
        for v in VECTORS.iter() {
            let username = "test@example.com";
            let salt = hex_to_bytes(v.salt);
            let a_secret = hex_to_bytes(v.a_secret);
            let b_pub = hex_to_bytes(v.b_pub);
            let derived_password = hex_to_bytes(v.derived_password);

            let client = SrpClient::new(username, &a_secret);
            assert_eq!(
                hex::encode(client.public_ephemeral()),
                v.a_pub,
                "A mismatch for {:?}",
                v.protocol
            );

            let result = client
                .process_challenge(&salt, &b_pub, &derived_password)
                .expect("challenge should be accepted");

            assert_eq!(hex::encode(&result.m1), v.m1, "M1 mismatch for {:?}", v.protocol);
            assert_eq!(
                hex::encode(&result.h_amk),
                v.h_amk,
                "H_AMK mismatch for {:?}",
                v.protocol
            );
        }
    }

    #[test]
    fn derive_password_matches_reference() {
        let salt = hex_to_bytes("0102030405060708090a0b0c0d0e0f10");
        let s2k = derive_password("correcthorsebatterystaple", &salt, 1000, 32, SrpProtocol::S2k);
        assert_eq!(
            hex::encode(&s2k),
            "4f84e41c94f96f6d7a5e1531d010f258ebb6803a50fec744d8d59ed4bcb03e64"
        );

        let s2k_fo = derive_password(
            "correcthorsebatterystaple",
            &salt,
            1000,
            32,
            SrpProtocol::S2kFo,
        );
        assert_eq!(
            hex::encode(&s2k_fo),
            "11bd3409dd3235b8bb7b850387390346b097bee2c1e71697a6d9b5f377e89658"
        );
    }
}
