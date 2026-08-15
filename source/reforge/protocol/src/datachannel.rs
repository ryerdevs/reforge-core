//! `protocol::datachannel` — canal de datos aditivo pull-based (F3 §5.6):
//! el cliente pide datos al server (manifest versionado + delta) y el server
//! responde; los paquetes son ADITIVOS (headers 162+) — el cliente legacy no
//! los envía/recibe aún (registro de contrato en PhaseLogin, no-op).
//!
//! Wire (little-endian, sin prefijo de longitud — el framing lo da la capa de
//! red como en el resto del crate; payload = bytes crudos de filas):
//!
//! - **162 `CG_QUERY`**: `BYTE header; BYTE table_id; payload[]` — el cliente
//!   pide una tabla/versión del manifest de datos.
//! - **163 `GC_RESPONSE`**: `BYTE header; BYTE table_id; WORD row_count;
//!   payload[]` — la respuesta del server (row_count u16 por si el payload se
//!   procesa por filas sin parsearlo).
//!
//! El plan §5.6 define el mecanismo (versioned manifest + delta, hot reload
//! vía LISTEN/NOTIFY) pero NO fija el wire — se adopta la forma mínima
//! (ponytail): id + count + filas crudas. El formato del payload (filas
//! serializadas, versiones) lo define el lane que implemente el servidor del
//! canal (F4+); aquí solo se fija el sobre.
//!
//! Parity del header en el cliente legacy: `Packet.h` no define 162/163 —
//! `PythonNetworkStreamPhaseLogin.cpp` registra los casos como no-op (contrato
//! F3 §5.6).

/// 162 — petición del cliente (client→server).
pub const CG_QUERY: u8 = 162;
/// 163 — respuesta del server (server→client).
pub const GC_RESPONSE: u8 = 163;

use crate::{ProtocolError, Result};

/// 162 — `CG_QUERY`: `header + table_id u8 + payload`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CgQuery {
    /// Id de tabla/entidad del manifest de datos (definido por el server del
    /// canal; sin semántica en el sobre).
    pub table_id: u8,
    /// Payload de la petición (bytes crudos; típicamente la versión pedida).
    pub payload: Vec<u8>,
}

impl CgQuery {
    /// Tamaño mínimo: header + table_id (payload puede ser vacío).
    pub const MIN_SIZE: usize = 2;

    pub fn new(table_id: u8, payload: Vec<u8>) -> Self {
        Self { table_id, payload }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::MIN_SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::MIN_SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            table_id: data[1],
            payload: data[2..].to_vec(),
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::MIN_SIZE + self.payload.len());
        b.push(CG_QUERY);
        b.push(self.table_id);
        b.extend_from_slice(&self.payload);
        b
    }
}

/// 163 — `GC_RESPONSE`: `header + table_id u8 + row_count u16 + payload`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcResponse {
    /// Id de tabla/entidad (eco del `CG_QUERY`).
    pub table_id: u8,
    /// Número de filas del payload (u16 — el delta de §5.6 es de KB).
    pub row_count: u16,
    /// Filas crudas serializadas por el server del canal.
    pub payload: Vec<u8>,
}

impl GcResponse {
    /// Tamaño mínimo: header + table_id + row_count (payload puede ser vacío).
    pub const MIN_SIZE: usize = 4;

    pub fn new(table_id: u8, row_count: u16, payload: Vec<u8>) -> Self {
        Self {
            table_id,
            row_count,
            payload,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::MIN_SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::MIN_SIZE,
                got: data.len(),
            });
        }
        let row_count = u16::from_le_bytes([data[2], data[3]]);
        Ok(Self {
            table_id: data[1],
            row_count,
            payload: data[4..].to_vec(),
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::MIN_SIZE + self.payload.len());
        b.push(GC_RESPONSE);
        b.push(self.table_id);
        b.extend_from_slice(&self.row_count.to_le_bytes());
        b.extend_from_slice(&self.payload);
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------ sizes

    #[test]
    fn encoded_sizes_are_min_plus_payload() {
        let q = CgQuery::new(7, vec![]);
        assert_eq!(q.to_bytes().len(), CgQuery::MIN_SIZE);
        let q = CgQuery::new(7, vec![0xAB; 300]);
        assert_eq!(q.to_bytes().len(), CgQuery::MIN_SIZE + 300);

        let r = GcResponse::new(7, 0, vec![]);
        assert_eq!(r.to_bytes().len(), GcResponse::MIN_SIZE);
        let r = GcResponse::new(7, 65535, vec![0xCD; 512]);
        assert_eq!(r.to_bytes().len(), GcResponse::MIN_SIZE + 512);
    }

    #[test]
    fn header_bytes_are_162_and_163() {
        assert_eq!(CgQuery::new(1, vec![]).to_bytes()[0], CG_QUERY);
        assert_eq!(CgQuery::new(1, vec![]).to_bytes()[0], 162);
        assert_eq!(GcResponse::new(1, 0, vec![]).to_bytes()[0], GC_RESPONSE);
        assert_eq!(GcResponse::new(1, 0, vec![]).to_bytes()[0], 163);
    }

    // ------------------------------------------------------------------ roundtrip

    #[test]
    fn cg_query_roundtrip() {
        for (table_id, payload) in [
            (0u8, vec![]),
            (255, vec![0x00]),
            (42, b"version=42".to_vec()),
            (1, vec![0xAA; 4096]),
        ] {
            let q = CgQuery::new(table_id, payload.clone());
            assert_eq!(
                CgQuery::from_bytes(&q.to_bytes()).unwrap(),
                q,
                "table_id={table_id}"
            );
        }
    }

    #[test]
    fn gc_response_roundtrip() {
        for (table_id, row_count, payload) in [
            (0u8, 0u16, vec![]),
            (255, 65535, vec![]),
            (3, 7, b"row1\x1frow2\x1frow3".to_vec()),
            (200, 1, vec![0x00, 0xFF, 0x10]),
        ] {
            let r = GcResponse::new(table_id, row_count, payload.clone());
            assert_eq!(
                GcResponse::from_bytes(&r.to_bytes()).unwrap(),
                r,
                "table_id={table_id} rows={row_count}"
            );
        }
    }

    #[test]
    fn row_count_is_le_at_offset_2() {
        // 163 + table_id=5 + row_count=0x1234 (LE) + payload
        let r = GcResponse::new(5, 0x1234, vec![0x99]);
        let b = r.to_bytes();
        assert_eq!(&b[..4], &[163, 5, 0x34, 0x12]);
    }

    // ------------------------------------------------------------------ bad lengths

    #[test]
    fn bad_lengths_are_errors() {
        for bad in [&[0u8; 0][..], &[0u8; 1][..]] {
            assert!(matches!(
                CgQuery::from_bytes(bad),
                Err(ProtocolError::BadLength { expected: 2, .. })
            ));
        }
        for bad in [&[0u8; 0][..], &[0u8; 1][..], &[0u8; 2][..], &[0u8; 3][..]] {
            assert!(matches!(
                GcResponse::from_bytes(bad),
                Err(ProtocolError::BadLength { expected: 4, .. })
            ));
        }
        // Boundary: exactamente MIN_SIZE es válido (payload vacío).
        assert_eq!(
            CgQuery::from_bytes(&[162, 9]).unwrap().payload,
            Vec::<u8>::new()
        );
        assert_eq!(
            GcResponse::from_bytes(&[163, 9, 0, 0]).unwrap().payload,
            Vec::<u8>::new()
        );
        // Payloads arbitrarios se conservan byte a byte.
        let q = CgQuery::from_bytes(&[162, 1, 0xDE, 0xAD, 0xBE, 0xEF]).unwrap();
        assert_eq!((q.table_id, q.payload), (1, vec![0xDE, 0xAD, 0xBE, 0xEF]));
    }
}
