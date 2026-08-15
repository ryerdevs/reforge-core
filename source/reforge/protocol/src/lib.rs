//! # `protocol` — paquetes byte-exactos del wire de Metin2 (cliente↔servidor).
//!
//! Contrato: `docs/reference/protocol/login-flow.md` §1–§3 (spec canónico;
//! el draft anterior `docs/superpowers/specs/2026-08-08-wire-protocol-login-flow.md`
//! quedó archivado como histórico — no usar).
//! Little-endian, sin prefijo de longitud, tamaños fijos (packed, sin padding).
//! Zero-deps: serialización manual LE (ADR-0003). Std only.
//!
//! Todos los structs se verificaron contra el código legacy:
//! - `source/server/game/src/packet.h`, `packet_info.cpp` (game server)
//! - `source/server/common/tables.h`, `length.h` (TSimplePlayer, constantes)
//! - `source/client/UserInterface/Packet.h` (cliente)
//! y empíricamente compilando los structs C reales con gcc -m32 (toolchain del
//! server) y MSVC 14.51 x86 (toolchain del cliente).
//!
//! # Nota sobre tamaños packed (ERRATA del spec, corregida 2026-08-10, spec §7)
//!
//! `TSimplePlayer`/`TAccountTable` están DENTRO de la región `#pragma pack(1)`
//! (`source/server/common/tables.h:271` abre; `:1333` cierra; el struct está en
//! `:285`) → **71 B packed** por jugador; `TPacketGCLoginSuccess` = **449 B**
//! (handle@441, random_key@445); `TAccountTable` = 444 B. El spec canónico ya
//! refleja estos tamaños; este crate implementa el formato wire real
//! (ver tests `spec_note_*`). Evidencia: gcc -m32 → `sizeof=71`, MSVC x86 →
//! `sizeof=71`, y el cliente registra `sizeof(TPacketGCLoginSuccess4)` = 449
//! (header 0x20) con login funcionando en producción.

// ============================================================================
// Constantes (spec §1 + packet.h verificado)
// ============================================================================

pub mod combat;
/// Paquetes legacy-client-only (ADR-0006): PanamaPack 151 + hybrid-crypt
/// 152/153 — el auth C++ los envía en login exitoso antes de `GC_AUTH_SUCCESS`
/// (`input_db.cpp:1710-1716`). Boundary aislado y borrable en bloque en F7.
pub mod legacy;
pub mod movement;
pub mod world;

/// Canal de datos aditivo pull-based (F3 §5.6): `CG_QUERY` (162) /
/// `GC_RESPONSE` (163) — manifest versionado + delta (server = única fuente
/// de datos). Aditivo: el cliente legacy registra los headers como no-op en
/// PhaseLogin; el wire del sobre se fija aquí (payload crudo, ver módulo).
pub mod datachannel;

/// F1 — locale server-side (ADR-0009): `CG_LOCALE_REQUEST` (132) /
/// `GC_LOCALE` (140) — el bundle de texto del cliente por idioma, chunked.
/// Aditivo (patrón datachannel); spec `docs/plans/locale-redesign.md` §Wire.
pub mod locale;

/// Chat y whisper (parity `packet.h` — `TPacketCGWhisper`/`TPacketGCWhisper`,
/// `length.h:19` `CHARACTER_NAME_MAX_LEN = 24`): los tamaños FIJOS del wire
/// (packed, LE) y las constantes de tipo.
///
/// - `TPacketCGWhisper` (C→S, 19) = BYTE header + WORD wSize + char
///   szNameTo[25] → **28 B**; `wSize` es el tamaño TOTAL (28 + mensaje),
///   parity `input_main.cpp:273-286` (`iExtraLen = wSize - sizeof`).
/// - `TPacketGCWhisper` (S→C, 34) = BYTE header + WORD wSize + BYTE bType +
///   char szNameFrom[25] → **29 B**; el mensaje viaja DESPUÉS (sin NUL — el
///   C++ lo corta con `strlen`, input_main.cpp:432-450).
pub mod chat {
    /// Tamaño fijo de `TPacketCGWhisper` (19): header + wSize + szNameTo[25].
    pub const CG_WHISPER_FIXED: usize = 28;
    /// Tamaño fijo de `TPacketGCWhisper` (34): header + wSize + bType + szNameFrom[25].
    pub const GC_WHISPER_FIXED: usize = 29;
    /// Bytes del campo nombre (`CHARACTER_NAME_MAX_LEN = 24` + NUL).
    pub const NAME_BYTES: usize = 25;
    /// `EChatType` (length.h:258-274): TALKING — el broadcast en rango.
    pub const TYPE_TALKING: u8 = 0;
    /// `EChatType` (length.h:258-274): SHOUT — el broadcast de mapa.
    pub const TYPE_SHOUT: u8 = 6;
    /// `EWhisperType` (packet.h): whisper normal (el cliente lo pinta).
    pub const WHISPER_CHAT: u8 = 0;
    /// `EWhisperType` (packet.h): el destino no existe (parity
    /// input_main.cpp:322-335 — sin mensaje).
    pub const WHISPER_NOT_EXIST: u8 = 1;
}

pub mod header {
    //! Headers de paquete (verificados contra `game/src/packet.h`).

    // C→S
    pub const CG_HANDSHAKE: u8 = 0xff;
    pub const CG_PONG: u8 = 0xfe;
    pub const CG_TIME_SYNC: u8 = 0xfc;
    pub const CG_LOGIN: u8 = 1;
    pub const CG_CHARACTER_CREATE: u8 = 4;
    pub const CG_CHARACTER_DELETE: u8 = 5;
    pub const CG_CHARACTER_SELECT: u8 = 6;
    pub const CG_ENTERGAME: u8 = 10;
    // Fase de juego (tabla C→S del framer — tamaños del Packet.h del cliente):
    pub const CG_ATTACK: u8 = 2;
    /// Variable (iExtraLen) — no parseable por el framer (cierre documentado).
    pub const CG_CHAT: u8 = 3;
    pub const CG_MOVE: u8 = 7;
    /// Variable (count + elements) — no parseable por el framer.
    pub const CG_SYNC_POSITION: u8 = 8;
    pub const CG_ITEM_USE: u8 = 11;
    pub const CG_ITEM_DROP: u8 = 12;
    pub const CG_ITEM_MOVE: u8 = 13;
    pub const CG_ITEM_PICKUP: u8 = 15;
    pub const CG_QUICKSLOT_ADD: u8 = 16;
    pub const CG_QUICKSLOT_DEL: u8 = 17;
    pub const CG_QUICKSLOT_SWAP: u8 = 18;
    /// Variable (nombre + mensaje) — no parseable por el framer.
    pub const CG_WHISPER: u8 = 19;
    pub const CG_ITEM_DROP2: u8 = 20;
    pub const CG_ON_CLICK: u8 = 26;
    pub const CG_EXCHANGE: u8 = 27;
    pub const CG_CHARACTER_POSITION: u8 = 28;
    pub const CG_SCRIPT_ANSWER: u8 = 29;
    pub const CG_QUEST_INPUT_STRING: u8 = 30;
    pub const CG_QUEST_CONFIRM: u8 = 31;
    pub const CG_PVP: u8 = 41;
    /// Tienda NPC (`Packet.h:641-645` TPacketCGShop): variable (2 B base +
    /// payload según subheader END/BUY/SELL/SELL2) — el framer lo resuelve
    /// por subheader (parity `input_main.cpp:1054-1088`).
    pub const CG_SHOP: u8 = 50;
    pub const CG_FLY_TARGETING: u8 = 51;
    pub const CG_USE_SKILL: u8 = 52;
    /// `HEADER_CG_ADD_FLY_TARGETING` (Packet.h:65 — 53): añadir un fly
    /// targeting (flecha/área de hechizo) — `TPacketCGFlyTargeting` 13 B
    /// (header+dwTargetVID+x+y; Packet.h:717-723). Distinto de
    /// `CG_FLY_TARGETING` (51, con shooter).
    pub const CG_ADD_FLY_TARGETING: u8 = 53;
    pub const CG_SHOOT: u8 = 54;
    pub const CG_MYSHOP: u8 = 55;
    pub const CG_ITEM_USE_TO_ITEM: u8 = 60;
    pub const CG_TARGET: u8 = 61;
    pub const CG_WARP: u8 = 65;
    pub const CG_SCRIPT_BUTTON: u8 = 66;
    /// `HEADER_CG_MESSENGER` (Packet.h:79 — 67): messenger (amigos) —
    /// `TPacketCGMessenger` 2 B (header+subheader; Packet.h:801-805).
    pub const CG_MESSENGER: u8 = 67;
    /// `HEADER_CG_MALL_CHECKOUT` (Packet.h:81 — 69): compra en la tienda
    /// (mall) — `TPacketCGMallCheckout` 5 B (header+bMallPos+TItemPos;
    /// Packet.h:839-845).
    pub const CG_MALL_CHECKOUT: u8 = 69;
    /// `HEADER_CG_SAFEBOX_CHECKIN` (Packet.h:82 — 70): meter un item en la
    /// safebox — `TPacketCGSafeboxCheckin` 5 B (header+bSafePos+TItemPos;
    /// Packet.h:832-838).
    pub const CG_SAFEBOX_CHECKIN: u8 = 70;
    /// `HEADER_CG_SAFEBOX_CHECKOUT` (Packet.h:83 — 71): sacar un item de la
    /// safebox — `TPacketCGSafeboxCheckout` 5 B (header+bSafePos+TItemPos;
    /// Packet.h:825-831).
    pub const CG_SAFEBOX_CHECKOUT: u8 = 71;
    /// `HEADER_CG_PARTY_INVITE` (Packet.h:84 — 72) — `TPacketCGPartyInvite`
    /// 5 B (header+vid; Packet.h:856-860).
    pub const CG_PARTY_INVITE: u8 = 72;
    /// `HEADER_CG_PARTY_INVITE_ANSWER` (Packet.h:85 — 73) —
    /// `TPacketCGPartyInviteAnswer` 6 B (header+leader_pid+accept;
    /// Packet.h:862-867).
    pub const CG_PARTY_INVITE_ANSWER: u8 = 73;
    /// `HEADER_CG_PARTY_REMOVE` (Packet.h:86 — 74) — `TPacketCGPartyRemove`
    /// 5 B (header+pid; Packet.h:869-873).
    pub const CG_PARTY_REMOVE: u8 = 74;
    /// `HEADER_CG_PARTY_SET_STATE` (Packet.h:87 — 75) —
    /// `TPacketCGPartySetState` 7 B (header+dwVID+byState+byFlag;
    /// Packet.h:875-881).
    pub const CG_PARTY_SET_STATE: u8 = 75;
    /// `HEADER_CG_PARTY_USE_SKILL` (Packet.h:88 — 76) —
    /// `TPacketCGPartyUseSkill` 6 B (header+bySkillIndex+dwTargetVID;
    /// Packet.h:897-902).
    pub const CG_PARTY_USE_SKILL: u8 = 76;
    /// `HEADER_CG_SAFEBOX_ITEM_MOVE` (Packet.h:89 — 77): mover un item
    /// dentro de la safebox — `TPacketCGItemMove` 8 B (mismo shape que
    /// `CG_ITEM_MOVE` 13; Packet.h:593-599).
    pub const CG_SAFEBOX_ITEM_MOVE: u8 = 77;
    /// `HEADER_CG_PARTY_PARAMETER` (Packet.h:90 — 78) —
    /// `TPacketCGPartyParameter` 2 B (header+bDistributeMode;
    /// Packet.h:1012-1016).
    pub const CG_PARTY_PARAMETER: u8 = 78;
    /// `HEADER_CG_GUILD` (Packet.h:92 — 80): guild — `TPacketCGGuild` 2 B
    /// (header+subheader; Packet.h:923-927).
    pub const CG_GUILD: u8 = 80;
    /// `HEADER_CG_ANSWER_MAKE_GUILD` (Packet.h:93 — 81): respuesta a la
    /// oferta de crear guild — `TPacketCGAnswerMakeGuild` 14 B
    /// (header+guild_name[13]; `GUILD_NAME_MAX_LEN`=12; Packet.h:929-933).
    pub const CG_ANSWER_MAKE_GUILD: u8 = 81;
    /// `HEADER_CG_FISHING` (Packet.h:94 — 82): pescar — `TPacketCGFishing`
    /// 2 B (header+dir; `packet.h:1800-1804` del server — el Packet.h del
    /// cliente solo define el GC).
    pub const CG_FISHING: u8 = 82;
    /// `HEADER_CG_ITEM_GIVE` (`packet.h:72` — 83; el Packet.h del cliente no
    /// lo define): dar un item a otro jugador — `TPacketCGGiveItem` 9 B
    /// (header+dwTargetVID+TItemPos+byItemCount; Packet.h:935-941).
    pub const CG_ITEM_GIVE: u8 = 83;
    /// `HEADER_CG_REFINE` (Packet.h:108 — 96): refinar item —
    /// `TPacketCGRefine` 3 B (header+pos+type; Packet.h:976-982).
    pub const CG_REFINE: u8 = 96;
    /// `HEADER_CG_HACK` (Packet.h:120 — 105): reporte de cheat del cliente —
    /// `TPacketCGHack` 257 B (header+szBuf[256]; Packet.h:943-947).
    pub const CG_HACK: u8 = 105;
    /// `HEADER_CG_SCRIPT_SELECT_ITEM` (Packet.h:126 — 114): selección de un
    /// item en un script — `TPacketCGScriptSelectItem` 5 B
    /// (header+selection; Packet.h:1031-1035).
    pub const CG_SCRIPT_SELECT_ITEM: u8 = 114;
    /// `HEADER_CG_DRAGON_SOUL_REFINE` (Packet.h:134 — 205): refinar dragon
    /// soul — `TPacketCGDragonSoulRefine` 47 B (header+bSubType+
    /// TItemPos[15]; `DS_REFINE_WINDOW_MAX_NUM`=15 — GameType.h:191;
    /// Packet.h:2715-2722).
    pub const CG_DRAGON_SOUL_REFINE: u8 = 205;
    /// `HEADER_CG_ACCE` (Packet.h:2752 — 211): acce (costume) — `SPacketAcce`
    /// 23 B (header+subheader+bWindow+dwPrice+bPos+tPos+dwItemVnum+
    /// dwMinAbs+dwMaxAbs; Packet.h:2765-2776).
    pub const CG_ACCE: u8 = 211;
    /// Ping del selector de canales (`ServerStateChecker.cpp:60`, `packet.h:97`);
    /// 1 byte (solo header). Lo usa la tabla de framing de `network`.
    pub const CG_STATE_CHECKER: u8 = 206;
    /// Login del guild mark (`packet.h:78`) — el cliente lo manda al entrar
    /// al mundo; el canal lo ignora (marks = F5).
    pub const CG_MARK_LOGIN: u8 = 100;
    /// Índice de marks del guild mark downloader (`packet.h:116` —
    /// `HEADER_CG_MARK_IDXLIST`): con el canal SIN handshake el downloader
    /// recibe GC_PHASE(LOGIN) directo y manda 0x68 como PRIMER paquete
    /// (`GuildMarkDownloader.cpp` `__LoginState_RecvPhase` → TODO_RECV_MARK
    /// → `__SendMarkIDXList`). El canal normal lo cierra sin responder
    /// (parity `input.cpp:560-566` — sin mark server).
    pub const CG_MARK_IDXLIST: u8 = 104;
    /// Version del cliente al terminar la carga (`Packet.h:135` — 0xf1,
    /// `TPacketCGClientVersion2` 67 B): el canal lo ignora sin validar
    /// (parity `input.cpp:205-213`).
    pub const CG_CLIENT_VERSION2: u8 = 0xf1;
    pub const CG_LOGIN2: u8 = 109;
    pub const CG_LOGIN3: u8 = 111;
    /// F1 (locale, aditivo — ADR-0009): el cliente pide el bundle de texto
    /// al conectar al auth, ANTES del LOGIN3 (`CG_LOCALE_REQUEST`, 4 B).
    pub const CG_LOCALE_REQUEST: u8 = 132;

    // S→C
    pub const GC_CHARACTER_ADD: u8 = 1;
    /// LoginSuccess clásico (header 6, `HEADER_GC_LOGIN_SUCCESS`; cliente
    /// `HEADER_GC_LOGIN_SUCCESS3` = 3 jugadores). El desplegado es
    /// `GC_LOGIN_SUCCESS_NEWSLOT` (0x20, 5 jugadores).
    pub const GC_LOGIN_SUCCESS: u8 = 6;
    pub const GC_LOGIN_FAILURE: u8 = 7;
    pub const GC_PHASE: u8 = 0xfd;
    pub const GC_HANDSHAKE: u8 = 0xff;
    pub const GC_EMPIRE: u8 = 90;
    pub const GC_LOGIN_KEY: u8 = 118;
    pub const GC_CHAR_ADDITIONAL_INFO: u8 = 136;
    pub const GC_PING: u8 = 44;
    pub const GC_AUTH_SUCCESS: u8 = 150;
    /// `HEADER_GC_ATTACK` (server `packet.h:123`, struct del cliente
    /// `Packet.h:1936-1942` — 10 B). OJO: el cliente v24 no lo despacha ni el
    /// C++ del server lo manda (ver `protocol::combat` — la animación es
    /// predicción local); se implementa por contrato wire.
    pub const GC_ATTACK: u8 = 12;
    /// `HEADER_GC_DAMAGE_INFO` (server `packet.h:245`, cliente `Packet.h:274`
    /// — 10 B): el feedback visible del golpe (número de daño).
    pub const GC_DAMAGE_INFO: u8 = 135;
    /// `HEADER_GC_CHAT` (cliente `Packet.h:148`; server `packet.h` — 9 B +
    /// mensaje variable: `TPacketGCChat` header+size+type+dwVID+bEmpire).
    pub const GC_CHAT: u8 = 4;
    /// `HEADER_GC_WHISPER` (cliente `Packet.h:178`, server `packet.h:148` —
    /// 29 B fijos + mensaje variable: `TPacketGCWhisper`
    /// header+wSize+bType+szNameFrom[25]).
    pub const GC_WHISPER: u8 = 34;
    /// `HEADER_GC_ITEM_GROUND_ADD` (cliente `Packet.h:170`, server
    /// `packet.h:139` — 26): un item EN EL SUELO (drop).
    pub const GC_ITEM_GROUND_ADD: u8 = 26;
    /// `HEADER_GC_ITEM_GROUND_DEL` (cliente `Packet.h:171`, server
    /// `packet.h:140` — 27): quita un item del suelo (pickup).
    pub const GC_ITEM_GROUND_DEL: u8 = 27;
    /// `HEADER_GC_ITEM_OWNERSHIP` (cliente `Packet.h:175`, server
    /// `packet.h` — 31): el dueño de un item del suelo.
    pub const GC_ITEM_OWNERSHIP: u8 = 31;
    /// `HEADER_GC_ITEM_UPDATE` (cliente `Packet.h:169`, server
    /// `packet.h:137` — 25): el UPDATE de un item del inventario (cantidad
    /// al apilar — `AutoStackItem`).
    pub const GC_ITEM_UPDATE: u8 = 25;
    /// `HEADER_GC_ITEM_DEL` (cliente `Packet.h:165`, server `packet.h:135` —
    /// 20): el borrado de un item del inventario (el cliente lo registra con
    /// `sizeof(TPacketGCItemDelDeprecated)` — 42 B, PythonNetworkStream.cpp:71).
    pub const GC_ITEM_DEL: u8 = 20;
    /// `HEADER_GC_WARP` (cliente `Packet.h:199`, server `packet.h:169` —
    /// 65): el warp del jugador (revive en la ciudad / teletransporte).
    pub const GC_WARP: u8 = 65;
    /// `HEADER_GC_TARGET` (cliente `Packet.h:167`, server `packet.h:167` —
    /// 63): la barra de vida del objetivo (`TPacketGCTarget` 6 B — vid +
    /// bHPPercent). Parity `SetTarget`/`BroadcastTargetPacket`
    /// (char.cpp:5048-5143).
    pub const GC_TARGET: u8 = 63;
    /// `HEADER_GC_SHOP` (cliente `Packet.h:183`, server `packet.h:153` —
    /// 38): la tienda NPC (`TPacketGCShop` 4 B: header+WORD size+subheader;
    /// START payload items — ver `server_realms/channel/shop.rs`).
    pub const GC_SHOP: u8 = 38;
    /// `HEADER_GC_EXCHANGE` (cliente `Packet.h:188`, server `packet.h:158` —
    /// 42): el intercambio jugador↔jugador (`TPacketGCExchange` 47 B —
    /// `server_realms/channel/trade.rs`).
    pub const GC_EXCHANGE: u8 = 42;
    /// `HEADER_GC_AFFECT_ADD` (cliente `Packet.h:267`, server `packet.h:228`
    /// — 126): un affect activo (`TPacketGCAffectAdd` 22 B).
    pub const GC_AFFECT_ADD: u8 = 126;
    /// LoginSuccess "new slot" = 0x20 (server `HEADER_GC_LOGIN_SUCCESS_NEWSLOT`,
    /// cliente `HEADER_GC_LOGIN_SUCCESS4`).
    pub const GC_LOGIN_SUCCESS_NEWSLOT: u8 = 32;
    /// F1 (locale, aditivo — ADR-0009): `GC_LOCALE` chunked variable-length
    /// (el bundle reensamblado son ~1-2 MB — excede u16, por eso va chunked).
    pub const GC_LOCALE: u8 = 140;
    /// `HEADER_GC_RESPOND_CHANNELSTATUS` (cliente `Packet.h:312` — 210): la
    /// respuesta al CG_STATE_CHECKER del selector de canales — `[0xd2][nSize
    /// i32][n× TChannelStatus port u16+status u8][bSuccess 0x01]` (parity
    /// `input_db.cpp:2433-2461`). El cliente matchea por puerto
    /// (`ServerStateChecker::Update`).
    pub const GC_RESPOND_CHANNELSTATUS: u8 = 210;
}

/// `LOGIN_MAX_LEN` = 30 → buffers `[31]`.
pub const LOGIN_MAX_LEN: usize = 30;
/// `PASSWD_MAX_LEN` = 16 → buffers `[17]`.
pub const PASSWD_MAX_LEN: usize = 16;
/// `CHARACTER_NAME_MAX_LEN` = 24 → buffers `[25]`.
pub const CHARACTER_NAME_MAX_LEN: usize = 24;
/// `ACCOUNT_STATUS_MAX_LEN` = 8 → buffers `[9]`.
pub const ACCOUNT_STATUS_MAX_LEN: usize = 8;
/// `GUILD_NAME_MAX_LEN` = 12 (`length.h:35`) → buffers `[13]`.
pub const GUILD_NAME_MAX_LEN: usize = 12;
/// `PLAYER_PER_ACCOUNT` = 5 (`ENABLE_PLAYER_PER_ACCOUNT5`).
pub const PLAYER_PER_ACCOUNT: usize = 5;
/// `CHR_EQUIPPART_NUM` con `ENABLE_ACCE_COSTUME_SYSTEM` = 5 (ARMOR, WEAPON, HEAD,
/// HAIR, ACCE).
pub const CHR_EQUIPPART_NUM: usize = 5;

/// Fases de `TPacketGCPhase` (enum `EPhase`, `packet.h:788`).
pub mod phase {
    pub const CLOSE: u8 = 0;
    pub const HANDSHAKE: u8 = 1;
    pub const LOGIN: u8 = 2;
    pub const SELECT: u8 = 3;
    pub const LOADING: u8 = 4;
    pub const GAME: u8 = 5;
    pub const DEAD: u8 = 6;
    pub const CLIENT_CONNECTING: u8 = 7;
    pub const DBCLIENT: u8 = 8;
    pub const P2P: u8 = 9;
    pub const AUTH: u8 = 10;
}

// ============================================================================
// Error / helpers
// ============================================================================

/// Error de parseo de un paquete. Nunca se hace panic: longitud incorrecta → `Err`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    /// La longitud del slice no es la esperada para este paquete.
    /// Para `TPacketCGLogin3` se aceptan 65 (canal) o 68 (auth); `expected`
    /// reporta el tamaño base de 65 B y el doc del struct explica el sufijo.
    BadLength { expected: usize, got: usize },
}

impl core::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProtocolError::BadLength { expected, got } => {
                write!(f, "bad packet length: expected {expected} bytes, got {got}")
            }
        }
    }
}

impl std::error::Error for ProtocolError {}

pub type Result<T> = std::result::Result<T, ProtocolError>;

// Lectura LE sin panics: solo se llama tras comprobar `len() == SIZE`.
#[inline]
pub(crate) fn rd_u16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}
#[inline]
pub(crate) fn rd_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
#[inline]
pub(crate) fn rd_i32(b: &[u8], off: usize) -> i32 {
    i32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
#[inline]
pub(crate) fn rd_f32(b: &[u8], off: usize) -> f32 {
    f32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
#[inline]
pub(crate) fn rd_arr<const N: usize>(b: &[u8], off: usize) -> [u8; N] {
    let mut a = [0u8; N];
    a.copy_from_slice(&b[off..off + N]);
    a
}

#[inline]
pub(crate) fn wr_u16(b: &mut [u8], off: usize, v: u16) {
    b[off..off + 2].copy_from_slice(&v.to_le_bytes());
}
#[inline]
pub(crate) fn wr_u32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
#[inline]
pub(crate) fn wr_i32(b: &mut [u8], off: usize, v: i32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
#[inline]
pub(crate) fn wr_f32(b: &mut [u8], off: usize, v: f32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

/// Convierte `&str` a un buffer C zero-padded de `N` bytes (semántica `strlcpy`:
/// copia hasta `N-1` bytes y NUL-termina; el resto queda a cero).
/// Con `N = 0` devuelve un buffer vacío sin panic (`saturating_sub(1)`).
pub fn from_cstr<const N: usize>(s: &str) -> [u8; N] {
    let mut a = [0u8; N];
    let n = s.len().min(N.saturating_sub(1));
    a[..n].copy_from_slice(&s.as_bytes()[..n]);
    a
}

/// Bytes de un buffer C hasta el primer NUL (sin el NUL).
pub fn cstr_bytes<const N: usize>(b: &[u8; N]) -> &[u8] {
    let n = b.iter().position(|&c| c == 0).unwrap_or(N);
    &b[..n]
}

/// `&str` (lossy) de un buffer C: bytes hasta el primer NUL, UTF-8 lenient.
pub fn cstr_str<const N: usize>(b: &[u8; N]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(cstr_bytes(b))
}

// ============================================================================
// C→S: login / selección
// ============================================================================

/// `TPacketCGHandshake` (13 B, `header::CG_HANDSHAKE` = 0xff).
/// Layout: `BYTE bHeader; DWORD dwHandshake; DWORD dwTime; long lDelta`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketCGHandshake {
    pub header: u8,
    pub dw_handshake: u32,
    pub dw_time: u32,
    pub l_delta: i32,
}

impl TPacketCGHandshake {
    pub const SIZE: usize = 13;
    pub const HEADER: u8 = header::CG_HANDSHAKE;

    pub fn new(dw_handshake: u32, dw_time: u32, l_delta: i32) -> Self {
        Self {
            header: Self::HEADER,
            dw_handshake,
            dw_time,
            l_delta,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            dw_handshake: rd_u32(data, 1),
            dw_time: rd_u32(data, 5),
            l_delta: rd_i32(data, 9),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        wr_u32(&mut b, 1, self.dw_handshake);
        wr_u32(&mut b, 5, self.dw_time);
        wr_i32(&mut b, 9, self.l_delta);
        b
    }
}

/// `TPacketCGLogin` (49 B, `header::CG_LOGIN` = 1).
/// Layout: `BYTE header; char login[31]; char passwd[17]`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketCGLogin {
    pub header: u8,
    pub login: [u8; LOGIN_MAX_LEN + 1],
    pub passwd: [u8; PASSWD_MAX_LEN + 1],
}

impl TPacketCGLogin {
    pub const SIZE: usize = 49;
    pub const HEADER: u8 = header::CG_LOGIN;

    pub fn new(login: &str, passwd: &str) -> Self {
        Self {
            header: Self::HEADER,
            login: from_cstr(login),
            passwd: from_cstr(passwd),
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            login: rd_arr(data, 1),
            passwd: rd_arr(data, 32),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1..32].copy_from_slice(&self.login);
        b[32..49].copy_from_slice(&self.passwd);
        b
    }
}

/// `TPacketCGLogin2` (52 B, `header::CG_LOGIN2` = 109).
/// Layout: `BYTE header; char login[31]; DWORD dwLoginKey; DWORD adwClientKey[4]`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketCGLogin2 {
    pub header: u8,
    pub login: [u8; LOGIN_MAX_LEN + 1],
    pub dw_login_key: u32,
    pub adw_client_key: [u32; 4],
}

impl TPacketCGLogin2 {
    pub const SIZE: usize = 52;
    pub const HEADER: u8 = header::CG_LOGIN2;

    pub fn new(login: &str, dw_login_key: u32, adw_client_key: [u32; 4]) -> Self {
        Self {
            header: Self::HEADER,
            login: from_cstr(login),
            dw_login_key,
            adw_client_key,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            login: rd_arr(data, 1),
            dw_login_key: rd_u32(data, 32),
            adw_client_key: [
                rd_u32(data, 36),
                rd_u32(data, 40),
                rd_u32(data, 44),
                rd_u32(data, 48),
            ],
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1..32].copy_from_slice(&self.login);
        wr_u32(&mut b, 32, self.dw_login_key);
        for (i, k) in self.adw_client_key.iter().enumerate() {
            wr_u32(&mut b, 36 + i * 4, *k);
        }
        b
    }
}

/// `TPacketCGLogin3` (65 B en canal / 68 B en auth, `header::CG_LOGIN3` = 111).
/// Layout: `BYTE header; char login[31]; char passwd[17]; DWORD adwClientKey[4]`
/// + en auth `char szLanguage[3]` (p.ej. "es\0", `__LANGUAGE_SYSTEM__`).
///
/// Verificado en el cliente: `AccountConnector.cpp:158` (auth, 68 B) y
/// `PythonNetworkStreamPhaseLogin.cpp:251` (`sizeof - sizeof(szLanguage)` = 65 B
/// en canal). Server: `packet_info.cpp:157` `sizeof(TPacketCGLogin3) + (g_bAuthServer ? 3 : 0)`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketCGLogin3 {
    pub header: u8,
    pub login: [u8; LOGIN_MAX_LEN + 1],
    pub passwd: [u8; PASSWD_MAX_LEN + 1],
    pub adw_client_key: [u32; 4],
    /// Sufijo de 3 B SOLO del auth ("es\0"). En canal es 0-length (65 B totales).
    pub sz_language: [u8; 3],
    /// F2b (aditivo, auth): version del cliente (DWORD LE tras el lang) —
    /// `None` si el LOGIN3 es de 68 B (cliente actual).
    pub version: Option<u32>,
    /// F2b (aditivo, auth): hardware ID (16 B tras la version) — `None` si el
    /// LOGIN3 no la trae (68/72 B).
    pub hwid: Option<[u8; 16]>,
}

impl TPacketCGLogin3 {
    pub const SIZE_CHANNEL: usize = 65;
    pub const SIZE_AUTH: usize = 68;
    /// 68 + version[4] (F2b — version sin hwid).
    pub const SIZE_AUTH_VERSION: usize = 72;
    /// 72 + hwid[16] (F2b — version + hardware id).
    pub const SIZE_AUTH_FULL: usize = 88;
    pub const HEADER: u8 = header::CG_LOGIN3;

    /// Login3 para el canal (65 B, sin sufijo de idioma).
    pub fn new_channel(login: &str, passwd: &str, adw_client_key: [u32; 4]) -> Self {
        Self {
            header: Self::HEADER,
            login: from_cstr(login),
            passwd: from_cstr(passwd),
            adw_client_key,
            sz_language: [0; 3],
            version: None,
            hwid: None,
        }
    }

    /// Login3 para el auth (68 B, con sufijo de idioma "es\0", "de\0", ...).
    pub fn new_auth(login: &str, passwd: &str, adw_client_key: [u32; 4], lang: &str) -> Self {
        Self {
            header: Self::HEADER,
            login: from_cstr(login),
            passwd: from_cstr(passwd),
            adw_client_key,
            sz_language: from_cstr::<3>(lang),
            version: None,
            hwid: None,
        }
    }

    /// Acepta 65 B (canal, sin idioma), 68 B (auth, con idioma), 72 B (auth +
    /// version F2b) y 88 B (auth + version + hwid F2b).
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let base = |data: &[u8]| Self {
            header: data[0],
            login: rd_arr(data, 1),
            passwd: rd_arr(data, 32),
            adw_client_key: [
                rd_u32(data, 49),
                rd_u32(data, 53),
                rd_u32(data, 57),
                rd_u32(data, 61),
            ],
            sz_language: [0; 3],
            version: None,
            hwid: None,
        };
        match data.len() {
            Self::SIZE_CHANNEL => Ok(base(data)),
            Self::SIZE_AUTH => Ok(Self {
                sz_language: rd_arr(data, 65),
                ..base(data)
            }),
            Self::SIZE_AUTH_VERSION => {
                let mut auth = Self {
                    sz_language: rd_arr(data, 65),
                    ..base(data)
                };
                auth.version = Some(rd_u32(data, 68));
                Ok(auth)
            }
            Self::SIZE_AUTH_FULL => {
                let mut auth = Self {
                    sz_language: rd_arr(data, 65),
                    ..base(data)
                };
                auth.version = Some(rd_u32(data, 68));
                auth.hwid = Some(rd_arr(data, 72));
                Ok(auth)
            }
            got => Err(ProtocolError::BadLength {
                expected: Self::SIZE_CHANNEL,
                got,
            }),
        }
    }

    /// Serializa a 65 B (canal): descarta `sz_language`.
    pub fn to_bytes_channel(&self) -> [u8; Self::SIZE_CHANNEL] {
        let mut b = [0u8; Self::SIZE_CHANNEL];
        b[0] = self.header;
        b[1..32].copy_from_slice(&self.login);
        b[32..49].copy_from_slice(&self.passwd);
        for (i, k) in self.adw_client_key.iter().enumerate() {
            wr_u32(&mut b, 49 + i * 4, *k);
        }
        b
    }

    /// Serializa a 68 B (auth): incluye `sz_language`.
    pub fn to_bytes_auth(&self) -> [u8; Self::SIZE_AUTH] {
        let mut b = [0u8; Self::SIZE_AUTH];
        b[..Self::SIZE_CHANNEL].copy_from_slice(&self.to_bytes_channel());
        b[65..68].copy_from_slice(&self.sz_language);
        b
    }

    /// Serializa según el contexto que representa: canal (65 B) si `sz_language`
    /// está vacío, auth (68 B) si no.
    ///
    /// ⚠️ **OJO (footgun):** la decisión 65 vs 68 depende de `sz_language == [0; 3]`.
    /// Si construiste el paquete con `new_channel`/`new_auth` o parseaste bytes, el
    /// resultado es correcto; PERO un struct con `sz_language` accidentalmente no
    /// vacío serializará 68 B (auth) y viceversa. Para el **auth usa SIEMPRE
    /// `to_bytes_auth()` explícito** y para el **canal `to_bytes_channel()`** —
    /// nunca dependas de esta heurística en código de red.
    pub fn to_bytes(&self) -> Vec<u8> {
        if self.sz_language == [0; 3] {
            self.to_bytes_channel().to_vec()
        } else {
            self.to_bytes_auth().to_vec()
        }
    }

    /// Serializa el LOGIN3 auth extendido (F2b): 68 B base + `version`[4] (si
    /// `Some`) + `hwid`[16] (si `Some`) → 68/72/88 B. Para el f16_peer y tests.
    pub fn to_bytes_auth_with(&self, version: Option<u32>, hwid: Option<[u8; 16]>) -> Vec<u8> {
        let mut b = self.to_bytes_auth().to_vec();
        if let Some(v) = version {
            b.extend_from_slice(&v.to_le_bytes());
        }
        if let Some(h) = hwid {
            b.extend_from_slice(&h);
        }
        b
    }
}

/// `TPacketCGPlayerSelect` (2 B, `header::CG_CHARACTER_SELECT` = 6).
/// Layout: `BYTE header; BYTE index`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketCGPlayerSelect {
    pub header: u8,
    pub index: u8,
}

impl TPacketCGPlayerSelect {
    pub const SIZE: usize = 2;
    pub const HEADER: u8 = header::CG_CHARACTER_SELECT;

    pub fn new(index: u8) -> Self {
        Self {
            header: Self::HEADER,
            index,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            index: data[1],
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        [self.header, self.index]
    }
}

/// `TPacketCGPlayerDelete` (10 B, `header::CG_CHARACTER_DELETE` = 5).
/// Layout: `BYTE header; BYTE index; char private_code[8]`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketCGPlayerDelete {
    pub header: u8,
    pub index: u8,
    pub private_code: [u8; 8],
}

impl TPacketCGPlayerDelete {
    pub const SIZE: usize = 10;
    pub const HEADER: u8 = header::CG_CHARACTER_DELETE;

    pub fn new(index: u8, private_code: &str) -> Self {
        Self {
            header: Self::HEADER,
            index,
            private_code: from_cstr(private_code),
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            index: data[1],
            private_code: rd_arr(data, 2),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1] = self.index;
        b[2..10].copy_from_slice(&self.private_code);
        b
    }
}

/// `TPacketCGPlayerCreate` (34 B, `header::CG_CHARACTER_CREATE` = 4).
/// Layout: `BYTE header; BYTE index; char name[25]; WORD job; BYTE shape;
/// BYTE Con; BYTE Int; BYTE Str; BYTE Dex`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketCGPlayerCreate {
    pub header: u8,
    pub index: u8,
    pub name: [u8; CHARACTER_NAME_MAX_LEN + 1],
    pub job: u16,
    pub shape: u8,
    pub con: u8,
    pub int_: u8,
    pub str_: u8,
    pub dex: u8,
}

impl TPacketCGPlayerCreate {
    pub const SIZE: usize = 34;
    pub const HEADER: u8 = header::CG_CHARACTER_CREATE;

    pub fn new(name: &str, job: u16, shape: u8, con: u8, int_: u8, str_: u8, dex: u8) -> Self {
        Self {
            header: Self::HEADER,
            index: 0,
            name: from_cstr(name),
            job,
            shape,
            con,
            int_,
            str_,
            dex,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            index: data[1],
            name: rd_arr(data, 2),
            job: rd_u16(data, 27),
            shape: data[29],
            con: data[30],
            int_: data[31],
            str_: data[32],
            dex: data[33],
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1] = self.index;
        b[2..27].copy_from_slice(&self.name);
        wr_u16(&mut b, 27, self.job);
        b[29] = self.shape;
        b[30] = self.con;
        b[31] = self.int_;
        b[32] = self.str_;
        b[33] = self.dex;
        b
    }
}

// ============================================================================
// S→C: handshake / fases / auth
// ============================================================================

/// `TPacketGCHandshake` (13 B, `header::GC_HANDSHAKE` = 0xff).
/// Layout idéntico a `TPacketCGHandshake`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCHandshake {
    pub header: u8,
    pub dw_handshake: u32,
    pub dw_time: u32,
    pub l_delta: i32,
}

impl TPacketGCHandshake {
    pub const SIZE: usize = 13;
    pub const HEADER: u8 = header::GC_HANDSHAKE;

    pub fn new(dw_handshake: u32, dw_time: u32, l_delta: i32) -> Self {
        Self {
            header: Self::HEADER,
            dw_handshake,
            dw_time,
            l_delta,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            dw_handshake: rd_u32(data, 1),
            dw_time: rd_u32(data, 5),
            l_delta: rd_i32(data, 9),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        wr_u32(&mut b, 1, self.dw_handshake);
        wr_u32(&mut b, 5, self.dw_time);
        wr_i32(&mut b, 9, self.l_delta);
        b
    }
}

/// `TPacketGCLoginKey` (5 B, `header::GC_LOGIN_KEY` = 118).
/// Layout: `BYTE bHeader; DWORD dwLoginKey`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCLoginKey {
    pub header: u8,
    pub dw_login_key: u32,
}

impl TPacketGCLoginKey {
    pub const SIZE: usize = 5;
    pub const HEADER: u8 = header::GC_LOGIN_KEY;

    pub fn new(dw_login_key: u32) -> Self {
        Self {
            header: Self::HEADER,
            dw_login_key,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            dw_login_key: rd_u32(data, 1),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        wr_u32(&mut b, 1, self.dw_login_key);
        b
    }
}

/// `TPacketGCPhase` (2 B, `header::GC_PHASE` = 0xfd).
/// Layout: `BYTE header; BYTE phase` (ver módulo [`phase`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCPhase {
    pub header: u8,
    pub phase: u8,
}

impl TPacketGCPhase {
    pub const SIZE: usize = 2;
    pub const HEADER: u8 = header::GC_PHASE;

    pub fn new(phase: u8) -> Self {
        Self {
            header: Self::HEADER,
            phase,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            phase: data[1],
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        [self.header, self.phase]
    }
}

/// `TPacketGCAuthSuccess` (6 B, `header::GC_AUTH_SUCCESS` = 150).
/// Layout: `BYTE bHeader; DWORD dwLoginKey; BYTE bResult`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCAuthSuccess {
    pub header: u8,
    pub dw_login_key: u32,
    pub b_result: u8,
}

impl TPacketGCAuthSuccess {
    pub const SIZE: usize = 6;
    pub const HEADER: u8 = header::GC_AUTH_SUCCESS;

    pub fn new(dw_login_key: u32, b_result: u8) -> Self {
        Self {
            header: Self::HEADER,
            dw_login_key,
            b_result,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            dw_login_key: rd_u32(data, 1),
            b_result: data[5],
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        wr_u32(&mut b, 1, self.dw_login_key);
        b[5] = self.b_result;
        b
    }
}

/// `TPacketGCLoginFailure` (10 B, `header::GC_LOGIN_FAILURE` = 7).
/// Layout: `BYTE header; char szStatus[9]`.
/// Status: "NOID", "WRONGPWD", "ALREADY", "NOTAVAIL", "BLKLOGIN", "VERSION",
/// "FULL", "SHUTDOWN".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCLoginFailure {
    pub header: u8,
    pub sz_status: [u8; ACCOUNT_STATUS_MAX_LEN + 1],
}

impl TPacketGCLoginFailure {
    pub const SIZE: usize = 10;
    pub const HEADER: u8 = header::GC_LOGIN_FAILURE;

    pub fn new(status: &str) -> Self {
        Self {
            header: Self::HEADER,
            sz_status: from_cstr(status),
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            sz_status: rd_arr(data, 1),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1..10].copy_from_slice(&self.sz_status);
        b
    }

    /// Status como `&str` (hasta el primer NUL).
    pub fn status(&self) -> std::borrow::Cow<'_, str> {
        cstr_str(&self.sz_status)
    }
}

/// `TPacketGCEmpire` (2 B, `header::GC_EMPIRE` = 90).
/// Layout: `BYTE bHeader; BYTE bEmpire` (1..3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCEmpire {
    pub header: u8,
    pub b_empire: u8,
}

impl TPacketGCEmpire {
    pub const SIZE: usize = 2;
    pub const HEADER: u8 = header::GC_EMPIRE;

    pub fn new(b_empire: u8) -> Self {
        Self {
            header: Self::HEADER,
            b_empire,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            b_empire: data[1],
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        [self.header, self.b_empire]
    }
}

// ============================================================================
// S→C: login success (TSimplePlayer) y spawn de personajes
// ============================================================================

/// `TSimplePlayer` (71 B, packed — ver nota del crate sobre la desviación del spec).
///
/// Layout packed (offsets verificados empíricamente, gcc -m32 y MSVC x86):
/// `DWORD dwID`(0) `char szName[25]`(4) `BYTE byJob`(29) `BYTE byLevel`(30)
/// `DWORD dwPlayMinutes`(31) `BYTE byST,byHT,byDX,byIQ`(35..39)
/// `DWORD wMainPart`(39) `BYTE bChangeName`(43) `DWORD wHairPart`(44)
/// `DWORD wAccePart`(48, ACCE ON) `BYTE bDummy[4]`(52) `long x,y`(56,60)
/// `long lAddr`(64) `WORD wPort`(68) `BYTE skill_group`(70).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TSimplePlayer {
    pub dw_id: u32,
    pub sz_name: [u8; CHARACTER_NAME_MAX_LEN + 1],
    pub by_job: u8,
    pub by_level: u8,
    pub dw_play_minutes: u32,
    pub by_st: u8,
    pub by_ht: u8,
    pub by_dx: u8,
    pub by_iq: u8,
    pub w_main_part: u32,
    pub b_change_name: u8,
    pub w_hair_part: u32,
    pub w_acce_part: u32,
    pub b_dummy: [u8; 4],
    pub x: i32,
    pub y: i32,
    pub l_addr: i32,
    pub w_port: u16,
    pub skill_group: u8,
}

impl TSimplePlayer {
    /// 71 B: 4+25+1+1+4+4+4+1+4+4+4+4+4+4+2+1 (packed, ACCE ON) — dwID(4) +
    /// szName(25) + byJob(1) + byLevel(1) + dwPlayMinutes(4) + byST..byIQ(4) +
    /// wMainPart(4) + bChangeName(1) + wHairPart(4) + wAccePart(4) + bDummy(4) +
    /// x(4) + y(4) + lAddr(4) + wPort(2) + skill_group(1) = 71.
    pub const SIZE: usize = 71;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            dw_id: rd_u32(data, 0),
            sz_name: rd_arr(data, 4),
            by_job: data[29],
            by_level: data[30],
            dw_play_minutes: rd_u32(data, 31),
            by_st: data[35],
            by_ht: data[36],
            by_dx: data[37],
            by_iq: data[38],
            w_main_part: rd_u32(data, 39),
            b_change_name: data[43],
            w_hair_part: rd_u32(data, 44),
            w_acce_part: rd_u32(data, 48),
            b_dummy: rd_arr(data, 52),
            x: rd_i32(data, 56),
            y: rd_i32(data, 60),
            l_addr: rd_i32(data, 64),
            w_port: rd_u16(data, 68),
            skill_group: data[70],
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        wr_u32(&mut b, 0, self.dw_id);
        b[4..29].copy_from_slice(&self.sz_name);
        b[29] = self.by_job;
        b[30] = self.by_level;
        wr_u32(&mut b, 31, self.dw_play_minutes);
        b[35] = self.by_st;
        b[36] = self.by_ht;
        b[37] = self.by_dx;
        b[38] = self.by_iq;
        wr_u32(&mut b, 39, self.w_main_part);
        b[43] = self.b_change_name;
        wr_u32(&mut b, 44, self.w_hair_part);
        wr_u32(&mut b, 48, self.w_acce_part);
        b[52..56].copy_from_slice(&self.b_dummy);
        wr_i32(&mut b, 56, self.x);
        wr_i32(&mut b, 60, self.y);
        wr_i32(&mut b, 64, self.l_addr);
        wr_u16(&mut b, 68, self.w_port);
        b[70] = self.skill_group;
        b
    }

    /// Nombre como `&str` (hasta el primer NUL).
    pub fn name(&self) -> std::borrow::Cow<'_, str> {
        cstr_str(&self.sz_name)
    }
}

/// `TPacketGCLoginSuccess` (449 B, `header::GC_LOGIN_SUCCESS_NEWSLOT` = 0x20).
///
/// Layout: `BYTE bHeader`(0) `TSimplePlayer players[5]`(1, 355 B)
/// `DWORD guild_id[5]`(356) `char guild_name[5][13]`(376)
/// `DWORD handle`(441) `DWORD random_key`(445) → total 449.
///
/// ⚠️ El spec dice 474 B con TSimplePlayer de 76 B (alineación natural). El wire
/// REAL es packed → 71 B/jugador → **449 B**. Ver nota del crate (evidencia:
/// compilación empírica gcc -m32 / MSVC x86, y `desc.cpp:987` +
/// `PythonNetworkStream.cpp:48` con login funcionando en producción).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCLoginSuccess {
    pub header: u8,
    pub players: [TSimplePlayer; PLAYER_PER_ACCOUNT],
    pub guild_id: [u32; PLAYER_PER_ACCOUNT],
    pub guild_name: [[u8; GUILD_NAME_MAX_LEN + 1]; PLAYER_PER_ACCOUNT],
    pub handle: u32,
    pub random_key: u32,
}

impl TPacketGCLoginSuccess {
    /// 1 + 5×71 + 5×4 + 5×13 + 4 + 4 = 449 (packed).
    pub const SIZE: usize = 449;
    pub const HEADER: u8 = header::GC_LOGIN_SUCCESS_NEWSLOT;
    /// Offset del primer `TSimplePlayer` dentro del paquete.
    pub const PLAYERS_OFFSET: usize = 1;
    /// Offset de `guild_id[0]` = 1 + 5×71.
    pub const GUILD_ID_OFFSET: usize = 356;
    /// Offset de `guild_name[0][0]` = 356 + 5×4.
    pub const GUILD_NAME_OFFSET: usize = 376;
    /// Offset de `handle` = 376 + 5×13.
    pub const HANDLE_OFFSET: usize = 441;
    /// Offset de `random_key`.
    pub const RANDOM_KEY_OFFSET: usize = 445;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        let mut players = [TSimplePlayer {
            dw_id: 0,
            sz_name: [0; 25],
            by_job: 0,
            by_level: 0,
            dw_play_minutes: 0,
            by_st: 0,
            by_ht: 0,
            by_dx: 0,
            by_iq: 0,
            w_main_part: 0,
            b_change_name: 0,
            w_hair_part: 0,
            w_acce_part: 0,
            b_dummy: [0; 4],
            x: 0,
            y: 0,
            l_addr: 0,
            w_port: 0,
            skill_group: 0,
        }; PLAYER_PER_ACCOUNT];
        for (i, p) in players.iter_mut().enumerate() {
            *p = TSimplePlayer::from_bytes(
                &data[Self::PLAYERS_OFFSET + i * 71..Self::PLAYERS_OFFSET + (i + 1) * 71],
            )?;
        }
        let mut guild_id = [0u32; PLAYER_PER_ACCOUNT];
        for (i, g) in guild_id.iter_mut().enumerate() {
            *g = rd_u32(data, Self::GUILD_ID_OFFSET + i * 4);
        }
        let mut guild_name = [[0u8; 13]; PLAYER_PER_ACCOUNT];
        for (i, g) in guild_name.iter_mut().enumerate() {
            *g = rd_arr(data, Self::GUILD_NAME_OFFSET + i * 13);
        }
        Ok(Self {
            header: data[0],
            players,
            guild_id,
            guild_name,
            handle: rd_u32(data, Self::HANDLE_OFFSET),
            random_key: rd_u32(data, Self::RANDOM_KEY_OFFSET),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        for (i, p) in self.players.iter().enumerate() {
            b[Self::PLAYERS_OFFSET + i * 71..Self::PLAYERS_OFFSET + (i + 1) * 71]
                .copy_from_slice(&p.to_bytes());
        }
        for (i, g) in self.guild_id.iter().enumerate() {
            wr_u32(&mut b, Self::GUILD_ID_OFFSET + i * 4, *g);
        }
        for (i, g) in self.guild_name.iter().enumerate() {
            b[Self::GUILD_NAME_OFFSET + i * 13..Self::GUILD_NAME_OFFSET + (i + 1) * 13]
                .copy_from_slice(g);
        }
        wr_u32(&mut b, Self::HANDLE_OFFSET, self.handle);
        wr_u32(&mut b, Self::RANDOM_KEY_OFFSET, self.random_key);
        b
    }
}

/// `TPacketGCCharacterAdd` (37 B, `header::GC_CHARACTER_ADD` = 1).
/// Layout: `BYTE header; DWORD dwVID; float angle; long x,y,z; BYTE bType;
/// DWORD wRaceNum; BYTE bMovingSpeed; BYTE bAttackSpeed; BYTE bStateFlag;
/// DWORD dwAffectFlag[2]`.
#[derive(Clone, Copy, PartialEq, Debug)]
#[repr(C)]
pub struct TPacketGCCharacterAdd {
    pub header: u8,
    pub dw_vid: u32,
    pub angle: f32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub b_type: u8,
    pub w_race_num: u32,
    pub b_moving_speed: u8,
    pub b_attack_speed: u8,
    pub b_state_flag: u8,
    pub dw_affect_flag: [u32; 2],
}

impl TPacketGCCharacterAdd {
    pub const SIZE: usize = 37;
    pub const HEADER: u8 = header::GC_CHARACTER_ADD;

    pub fn new(
        dw_vid: u32,
        angle: f32,
        x: i32,
        y: i32,
        z: i32,
        b_type: u8,
        w_race_num: u32,
        b_moving_speed: u8,
        b_attack_speed: u8,
        b_state_flag: u8,
        dw_affect_flag: [u32; 2],
    ) -> Self {
        Self {
            header: Self::HEADER,
            dw_vid,
            angle,
            x,
            y,
            z,
            b_type,
            w_race_num,
            b_moving_speed,
            b_attack_speed,
            b_state_flag,
            dw_affect_flag,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            dw_vid: rd_u32(data, 1),
            angle: rd_f32(data, 5),
            x: rd_i32(data, 9),
            y: rd_i32(data, 13),
            z: rd_i32(data, 17),
            b_type: data[21],
            w_race_num: rd_u32(data, 22),
            b_moving_speed: data[26],
            b_attack_speed: data[27],
            b_state_flag: data[28],
            dw_affect_flag: [rd_u32(data, 29), rd_u32(data, 33)],
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        wr_u32(&mut b, 1, self.dw_vid);
        wr_f32(&mut b, 5, self.angle);
        wr_i32(&mut b, 9, self.x);
        wr_i32(&mut b, 13, self.y);
        wr_i32(&mut b, 17, self.z);
        b[21] = self.b_type;
        wr_u32(&mut b, 22, self.w_race_num);
        b[26] = self.b_moving_speed;
        b[27] = self.b_attack_speed;
        b[28] = self.b_state_flag;
        wr_u32(&mut b, 29, self.dw_affect_flag[0]);
        wr_u32(&mut b, 33, self.dw_affect_flag[1]);
        b
    }
}

/// `TPacketGCCharacterAdditionalInfo` (70 B, `header::GC_CHAR_ADDITIONAL_INFO` = 136).
/// Layout: `BYTE header; DWORD dwVID; char name[25]; DWORD awPart[5];
/// BYTE bEmpire; DWORD dwGuildID; DWORD dwLevel; short sAlignment; BYTE bPKMode;
/// DWORD dwMountVnum; DWORD dwArrow` (QUIVER ON).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCCharacterAdditionalInfo {
    pub header: u8,
    pub dw_vid: u32,
    pub name: [u8; CHARACTER_NAME_MAX_LEN + 1],
    pub aw_part: [u32; CHR_EQUIPPART_NUM],
    pub b_empire: u8,
    pub dw_guild_id: u32,
    pub dw_level: u32,
    pub s_alignment: i16,
    pub b_pk_mode: u8,
    pub dw_mount_vnum: u32,
    pub dw_arrow: u32,
}

impl TPacketGCCharacterAdditionalInfo {
    pub const SIZE: usize = 70;
    pub const HEADER: u8 = header::GC_CHAR_ADDITIONAL_INFO;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        let mut aw_part = [0u32; CHR_EQUIPPART_NUM];
        for (i, p) in aw_part.iter_mut().enumerate() {
            *p = rd_u32(data, 30 + i * 4);
        }
        Ok(Self {
            header: data[0],
            dw_vid: rd_u32(data, 1),
            name: rd_arr(data, 5),
            aw_part,
            b_empire: data[50],
            dw_guild_id: rd_u32(data, 51),
            dw_level: rd_u32(data, 55),
            s_alignment: i16::from_le_bytes([data[59], data[60]]),
            b_pk_mode: data[61],
            dw_mount_vnum: rd_u32(data, 62),
            dw_arrow: rd_u32(data, 66),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        wr_u32(&mut b, 1, self.dw_vid);
        b[5..30].copy_from_slice(&self.name);
        for (i, p) in self.aw_part.iter().enumerate() {
            wr_u32(&mut b, 30 + i * 4, *p);
        }
        b[50] = self.b_empire;
        wr_u32(&mut b, 51, self.dw_guild_id);
        wr_u32(&mut b, 55, self.dw_level);
        b[59..61].copy_from_slice(&self.s_alignment.to_le_bytes());
        b[61] = self.b_pk_mode;
        wr_u32(&mut b, 62, self.dw_mount_vnum);
        wr_u32(&mut b, 66, self.dw_arrow);
        b
    }

    /// Nombre como `&str` (hasta el primer NUL).
    pub fn name(&self) -> std::borrow::Cow<'_, str> {
        cstr_str(&self.name)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // (a) Asserts de tamaño: wire size + size_of del struct Rust (repr(C))
    // ------------------------------------------------------------------
    // Los structs usan `#[repr(C)]` SIN packed: la serialización es manual, así
    // que el layout Rust es irrelevante para el wire (los tamaños exactos están
    // fijados por `SIZE` y verificados en `wire_sizes` + golden vectors). El
    // layout natural de Rust solo puede ser >= al wire (nunca menor: la suma de
    // campos contiguos es exactamente el wire size), de ahí el assert `>=`.

    macro_rules! size_asserts {
        ($t:ty) => {
            const _: () = assert!(core::mem::size_of::<$t>() >= <$t>::SIZE);
        };
    }
    size_asserts!(TPacketCGHandshake);
    size_asserts!(TPacketCGLogin);
    size_asserts!(TPacketCGLogin2);
    size_asserts!(TPacketCGPlayerSelect);
    size_asserts!(TPacketCGPlayerDelete);
    size_asserts!(TPacketCGPlayerCreate);
    size_asserts!(TPacketGCHandshake);
    size_asserts!(TPacketGCLoginKey);
    size_asserts!(TPacketGCPhase);
    size_asserts!(TPacketGCAuthSuccess);
    size_asserts!(TPacketGCLoginFailure);
    size_asserts!(TPacketGCEmpire);
    size_asserts!(TSimplePlayer);
    size_asserts!(TPacketGCLoginSuccess);
    size_asserts!(TPacketGCCharacterAdd);
    size_asserts!(TPacketGCCharacterAdditionalInfo);
    // Login3 no tiene `SIZE` único (65 canal / 68 auth); el struct Rust con lang = 68.
    const _: () = assert!(core::mem::size_of::<TPacketCGLogin3>() >= TPacketCGLogin3::SIZE_AUTH);

    #[test]
    fn wire_sizes() {
        // Valores del spec §3 (los que coinciden con el wire real).
        assert_eq!(TPacketCGHandshake::SIZE, 13);
        assert_eq!(TPacketCGLogin::SIZE, 49);
        assert_eq!(TPacketCGLogin2::SIZE, 52);
        assert_eq!(TPacketCGLogin3::SIZE_CHANNEL, 65);
        assert_eq!(TPacketCGLogin3::SIZE_AUTH, 68);
        assert_eq!(TPacketGCLoginKey::SIZE, 5);
        assert_eq!(TPacketCGPlayerSelect::SIZE, 2);
        assert_eq!(TPacketCGPlayerDelete::SIZE, 10);
        assert_eq!(TPacketCGPlayerCreate::SIZE, 34);
        assert_eq!(TPacketGCPhase::SIZE, 2);
        assert_eq!(TPacketGCHandshake::SIZE, 13);
        assert_eq!(TPacketGCAuthSuccess::SIZE, 6);
        assert_eq!(TPacketGCLoginFailure::SIZE, 10);
        assert_eq!(TPacketGCEmpire::SIZE, 2);
        assert_eq!(TPacketGCCharacterAdd::SIZE, 37);
        assert_eq!(TPacketGCCharacterAdditionalInfo::SIZE, 70);
        // Desviación documentada respecto al spec (474/76): el wire real es packed.
        assert_eq!(
            TSimplePlayer::SIZE,
            71,
            "spec dice 76 (natural); wire real = 71 (packed)"
        );
        assert_eq!(
            TPacketGCLoginSuccess::SIZE,
            449,
            "spec dice 474; wire real = 449 (packed)"
        );
    }

    #[test]
    fn spec_note_login_success_offsets() {
        // Verifica los offsets documentados en el spec §3 para TPacketGCLoginSuccess,
        // calculados con el layout REAL (packed 71 B):
        // players[0] @ 1, guild_id[0] @ 356, guild_name[0] @ 376, handle @ 441, random @ 445.
        assert_eq!(TPacketGCLoginSuccess::PLAYERS_OFFSET, 1);
        assert_eq!(TPacketGCLoginSuccess::GUILD_ID_OFFSET, 1 + 5 * 71);
        assert_eq!(TPacketGCLoginSuccess::GUILD_NAME_OFFSET, 1 + 5 * 71 + 5 * 4);
        assert_eq!(
            TPacketGCLoginSuccess::HANDLE_OFFSET,
            1 + 5 * 71 + 5 * 4 + 5 * 13
        );
        assert_eq!(TPacketGCLoginSuccess::RANDOM_KEY_OFFSET, 449 - 4);
        // El spec afirma estos offsets para TSimplePlayer de 76 B: si algún día se
        // cambia a natural alignment, estos asserts saltarán.
        assert_eq!(TSimplePlayer::SIZE, 71);
    }

    // ------------------------------------------------------------------
    // (b) Roundtrips parse → serialize → parse para cada paquete
    // ------------------------------------------------------------------

    #[test]
    fn roundtrip_cg_handshake() {
        let p = TPacketCGHandshake::new(0x1122_3344, 0x0102_0304, -100);
        let b = p.to_bytes();
        assert_eq!(b.len(), 13);
        let p2 = TPacketCGHandshake::from_bytes(&b).unwrap();
        assert_eq!(p, p2);
        assert_eq!(p2.to_bytes(), b);
    }

    #[test]
    fn roundtrip_cg_login() {
        let p = TPacketCGLogin::new("test", "1234");
        let b = p.to_bytes();
        let p2 = TPacketCGLogin::from_bytes(&b).unwrap();
        assert_eq!(p, p2);
        assert_eq!(p2.to_bytes(), b);
        assert_eq!(p2.login, from_cstr::<31>("test"));
    }

    #[test]
    fn roundtrip_cg_login2() {
        let p = TPacketCGLogin2::new("test", 0xDEAD_BEEF, [1, 2, 3, 4]);
        let b = p.to_bytes();
        let p2 = TPacketCGLogin2::from_bytes(&b).unwrap();
        assert_eq!(p, p2);
        assert_eq!(p2.to_bytes(), b);
    }

    #[test]
    fn roundtrip_cg_login3_channel() {
        let p = TPacketCGLogin3::new_channel("test", "1234", [1, 2, 3, 4]);
        let b = p.to_bytes_channel();
        assert_eq!(b.len(), 65);
        let p2 = TPacketCGLogin3::from_bytes(&b).unwrap();
        assert_eq!(p, p2);
        assert_eq!(p2.to_bytes_channel(), b);
        assert_eq!(p2.sz_language, [0; 3]);
        // to_bytes() genérico → 65 (sin idioma)
        assert_eq!(p.to_bytes().len(), 65);
    }

    #[test]
    fn roundtrip_cg_login3_auth() {
        let p = TPacketCGLogin3::new_auth("test", "1234", [1, 2, 3, 4], "es");
        let b = p.to_bytes_auth();
        assert_eq!(b.len(), 68);
        let p2 = TPacketCGLogin3::from_bytes(&b).unwrap();
        assert_eq!(p, p2);
        assert_eq!(p2.to_bytes_auth(), b);
        assert_eq!(p2.sz_language, *b"es\0");
        assert_eq!(p2.version, None, "68 B sin version");
        assert_eq!(p2.hwid, None, "68 B sin hwid");
        // to_bytes() genérico → 68 (con idioma)
        assert_eq!(p.to_bytes().len(), 68);
    }

    /// F2b: LOGIN3 auth extendido — 72 B (con version) y 88 B (con hwid).
    #[test]
    fn roundtrip_cg_login3_auth_extended() {
        let hwid = [0x11u8; 16];
        // 72 B: version sin hwid.
        let mut b72 = TPacketCGLogin3::new_auth("test", "1234", [1, 2, 3, 4], "es")
            .to_bytes_auth()
            .to_vec();
        b72.extend(40999u32.to_le_bytes());
        assert_eq!(b72.len(), 72);
        let p72 = TPacketCGLogin3::from_bytes(&b72).unwrap();
        assert_eq!(p72.version, Some(40999));
        assert_eq!(p72.hwid, None);
        // 88 B: version + hwid.
        let mut b88 = b72.clone();
        b88.extend_from_slice(&hwid);
        assert_eq!(b88.len(), 88);
        let p88 = TPacketCGLogin3::from_bytes(&b88).unwrap();
        assert_eq!(p88.version, Some(40999));
        assert_eq!(p88.hwid, Some(hwid));
        // to_bytes_auth_with: 68/72/88.
        let p = TPacketCGLogin3::new_auth("test", "1234", [1, 2, 3, 4], "es");
        assert_eq!(p.to_bytes_auth_with(None, None).len(), 68);
        assert_eq!(p.to_bytes_auth_with(Some(40999), None).len(), 72);
        assert_eq!(p.to_bytes_auth_with(Some(40999), Some(hwid)).len(), 88);
        let round =
            TPacketCGLogin3::from_bytes(&p.to_bytes_auth_with(Some(40999), Some(hwid))).unwrap();
        assert_eq!(round, p88);
    }

    /// F2b: longitudes inválidas del LOGIN3 → error.
    #[test]
    fn cg_login3_bad_lengths() {
        for len in [0usize, 64, 66, 67, 69, 71, 73, 89] {
            let mut b = vec![0u8; len];
            if !b.is_empty() {
                b[0] = header::CG_LOGIN3;
            }
            assert!(
                TPacketCGLogin3::from_bytes(&b).is_err(),
                "len {len} debe fallar"
            );
        }
    }

    #[test]
    fn roundtrip_cg_player_select() {
        let p = TPacketCGPlayerSelect::new(0);
        let b = p.to_bytes();
        let p2 = TPacketCGPlayerSelect::from_bytes(&b).unwrap();
        assert_eq!(p, p2);
        assert_eq!(p2.to_bytes(), b);
    }

    #[test]
    fn roundtrip_cg_player_delete() {
        let p = TPacketCGPlayerDelete::new(3, "12345678");
        let b = p.to_bytes();
        let p2 = TPacketCGPlayerDelete::from_bytes(&b).unwrap();
        assert_eq!(p, p2);
        assert_eq!(p2.to_bytes(), b);
        assert_eq!(p2.private_code, from_cstr::<8>("12345678"));
    }

    #[test]
    fn roundtrip_cg_player_create() {
        let p = TPacketCGPlayerCreate::new("Warrior", 0, 1, 30, 20, 40, 10);
        let b = p.to_bytes();
        let p2 = TPacketCGPlayerCreate::from_bytes(&b).unwrap();
        assert_eq!(p, p2);
        assert_eq!(p2.to_bytes(), b);
        assert_eq!(p2.name, from_cstr::<25>("Warrior"));
    }

    #[test]
    fn roundtrip_gc_handshake() {
        let p = TPacketGCHandshake::new(0x1122_3344, 0x0102_0304, 100);
        let b = p.to_bytes();
        let p2 = TPacketGCHandshake::from_bytes(&b).unwrap();
        assert_eq!(p, p2);
        assert_eq!(p2.to_bytes(), b);
    }

    #[test]
    fn roundtrip_gc_login_key() {
        let p = TPacketGCLoginKey::new(0xCAFE_BABE);
        let b = p.to_bytes();
        let p2 = TPacketGCLoginKey::from_bytes(&b).unwrap();
        assert_eq!(p, p2);
        assert_eq!(p2.to_bytes(), b);
    }

    #[test]
    fn roundtrip_gc_phase() {
        for ph in [phase::HANDSHAKE, phase::LOGIN, phase::SELECT, phase::AUTH] {
            let p = TPacketGCPhase::new(ph);
            let b = p.to_bytes();
            let p2 = TPacketGCPhase::from_bytes(&b).unwrap();
            assert_eq!(p, p2);
            assert_eq!(p2.to_bytes(), b);
        }
    }

    #[test]
    fn roundtrip_gc_auth_success() {
        let p = TPacketGCAuthSuccess::new(0xDEAD_BEEF, 1);
        let b = p.to_bytes();
        let p2 = TPacketGCAuthSuccess::from_bytes(&b).unwrap();
        assert_eq!(p, p2);
        assert_eq!(p2.to_bytes(), b);
    }

    #[test]
    fn roundtrip_gc_login_failure() {
        for s in [
            "NOID", "WRONGPWD", "ALREADY", "NOTAVAIL", "BLKLOGIN", "VERSION", "FULL", "SHUTDOWN",
        ] {
            let p = TPacketGCLoginFailure::new(s);
            let b = p.to_bytes();
            let p2 = TPacketGCLoginFailure::from_bytes(&b).unwrap();
            assert_eq!(p, p2);
            assert_eq!(p2.to_bytes(), b);
            assert_eq!(p2.status(), s);
        }
    }

    #[test]
    fn roundtrip_gc_empire() {
        let p = TPacketGCEmpire::new(1);
        let b = p.to_bytes();
        let p2 = TPacketGCEmpire::from_bytes(&b).unwrap();
        assert_eq!(p, p2);
        assert_eq!(p2.to_bytes(), b);
    }

    #[test]
    fn roundtrip_gc_login_success() {
        let mut p = TPacketGCLoginSuccess {
            header: TPacketGCLoginSuccess::HEADER,
            players: [TSimplePlayer {
                dw_id: 0,
                sz_name: [0; 25],
                by_job: 0,
                by_level: 0,
                dw_play_minutes: 0,
                by_st: 0,
                by_ht: 0,
                by_dx: 0,
                by_iq: 0,
                w_main_part: 0,
                b_change_name: 0,
                w_hair_part: 0,
                w_acce_part: 0,
                b_dummy: [0; 4],
                x: 0,
                y: 0,
                l_addr: 0,
                w_port: 0,
                skill_group: 0,
            }; 5],
            guild_id: [0; 5],
            guild_name: [[0u8; 13]; 5],
            handle: 0,
            random_key: 0,
        };
        p.players[0].dw_id = 1;
        p.players[0].sz_name = from_cstr::<25>("Hercules");
        p.players[0].by_job = 0;
        p.players[0].by_level = 42;
        p.players[0].dw_play_minutes = 3600;
        p.players[0].by_st = 90;
        p.players[0].by_ht = 80;
        p.players[0].by_dx = 70;
        p.players[0].by_iq = 60;
        p.players[0].w_main_part = 0xAABB_CCDD;
        p.players[0].b_change_name = 0;
        p.players[0].w_hair_part = 0x1122_3344;
        p.players[0].w_acce_part = 0x5566_7788;
        p.players[0].b_dummy = [0xAB; 4];
        p.players[0].x = 969_600;
        p.players[0].y = 278_400;
        p.players[0].l_addr = 0x7F00_0001;
        p.players[0].w_port = 30_003;
        p.players[0].skill_group = 1;
        p.players[1].dw_id = 2;
        p.players[1].sz_name = from_cstr::<25>("Ninja");
        p.players[4].dw_id = 99;
        p.guild_id[0] = 500;
        p.guild_id[4] = 501;
        p.guild_name[0] = from_cstr::<13>("GuildA");
        p.guild_name[4] = from_cstr::<13>("GuildE");
        p.handle = 0xDEAD_BEEF;
        p.random_key = 0xCAFE_BABE;

        let b = p.to_bytes();
        assert_eq!(b.len(), 449);
        let p2 = TPacketGCLoginSuccess::from_bytes(&b).unwrap();
        assert_eq!(p, p2);
        assert_eq!(p2.to_bytes(), b);
        assert_eq!(p2.players[0].name(), "Hercules");
        assert_eq!(p2.players[1].name(), "Ninja");
        assert_eq!(p2.guild_name[0], from_cstr::<13>("GuildA"));
    }

    #[test]
    fn roundtrip_gc_character_add() {
        let p =
            TPacketGCCharacterAdd::new(0x1000, 1.5, 1000, -2000, 300, 2, 101, 80, 90, 0, [1, 2]);
        let b = p.to_bytes();
        let p2 = TPacketGCCharacterAdd::from_bytes(&b).unwrap();
        assert_eq!(p, p2);
        assert_eq!(p2.to_bytes(), b);
        assert_eq!(p2.angle, 1.5);
    }

    #[test]
    fn roundtrip_gc_char_additional_info() {
        let mut p = TPacketGCCharacterAdditionalInfo {
            header: TPacketGCCharacterAdditionalInfo::HEADER,
            dw_vid: 0x1000,
            name: [0; 25],
            aw_part: [0; 5],
            b_empire: 1,
            dw_guild_id: 500,
            dw_level: 42,
            s_alignment: 100,
            b_pk_mode: 0,
            dw_mount_vnum: 0,
            dw_arrow: 1234,
        };
        p.name = from_cstr::<25>("NPC_Farmer");
        p.aw_part = [1, 2, 3, 4, 5];
        let b = p.to_bytes();
        let p2 = TPacketGCCharacterAdditionalInfo::from_bytes(&b).unwrap();
        assert_eq!(p, p2);
        assert_eq!(p2.to_bytes(), b);
        assert_eq!(p2.name(), "NPC_Farmer");
        assert_eq!(p2.aw_part, [1, 2, 3, 4, 5]);
    }

    // ------------------------------------------------------------------
    // (c) Golden vectors — bytes esperados construidos MANUALMENTE desde el
    //     layout del spec (independientes de to_bytes()).
    // ------------------------------------------------------------------

    #[test]
    fn golden_cg_login3_channel_65b() {
        // Layout (spec §3): header=111(0x6F) | login[31] | passwd[17] | key[4]×u32 LE
        // login "test\0" @1..5; passwd "1234\0" @32..36; key [1,2,3,4] @49..65.
        let mut exp = [0u8; 65];
        exp[0] = 0x6F;
        exp[1..5].copy_from_slice(b"test");
        exp[32..36].copy_from_slice(b"1234");
        exp[49..53].copy_from_slice(&1u32.to_le_bytes());
        exp[53..57].copy_from_slice(&2u32.to_le_bytes());
        exp[57..61].copy_from_slice(&3u32.to_le_bytes());
        exp[61..65].copy_from_slice(&4u32.to_le_bytes());

        let p = TPacketCGLogin3::from_bytes(&exp).unwrap();
        assert_eq!(p.header, 111);
        assert_eq!(p.login, from_cstr::<31>("test"));
        assert_eq!(p.passwd, from_cstr::<17>("1234"));
        assert_eq!(p.adw_client_key, [1, 2, 3, 4]);
        assert_eq!(p.sz_language, [0; 3]);
        assert_eq!(p.to_bytes_channel(), exp);
        assert_eq!(p.to_bytes(), exp.to_vec());
    }

    #[test]
    fn golden_cg_login3_auth_68b() {
        // Igual que el de canal + szLanguage "es\0" @65..68.
        let mut exp = [0u8; 68];
        exp[0] = 0x6F;
        exp[1..5].copy_from_slice(b"test");
        exp[32..36].copy_from_slice(b"1234");
        exp[49..53].copy_from_slice(&1u32.to_le_bytes());
        exp[53..57].copy_from_slice(&2u32.to_le_bytes());
        exp[57..61].copy_from_slice(&3u32.to_le_bytes());
        exp[61..65].copy_from_slice(&4u32.to_le_bytes());
        exp[65..68].copy_from_slice(b"es\0");

        let p = TPacketCGLogin3::from_bytes(&exp).unwrap();
        assert_eq!(p.header, 111);
        assert_eq!(p.sz_language, *b"es\0");
        assert_eq!(p.to_bytes_auth(), exp);
        assert_eq!(p.to_bytes(), exp.to_vec());
    }

    #[test]
    fn golden_gc_handshake() {
        // Layout: 0xff | dwHandshake 0x11223344 LE | dwTime 0x01020304 LE | lDelta -100 LE
        let exp: [u8; 13] = [
            0xFF, 0x44, 0x33, 0x22, 0x11, 0x04, 0x03, 0x02, 0x01, 0x9C, 0xFF, 0xFF, 0xFF,
        ];
        let p = TPacketGCHandshake::from_bytes(&exp).unwrap();
        assert_eq!(p.header, 0xFF);
        assert_eq!(p.dw_handshake, 0x1122_3344);
        assert_eq!(p.dw_time, 0x0102_0304);
        assert_eq!(p.l_delta, -100);
        assert_eq!(p.to_bytes(), exp);
    }

    #[test]
    fn golden_gc_phase() {
        // Layout: 0xfd | phase
        let exp: [u8; 2] = [0xFD, 0x01]; // PHASE_HANDSHAKE
        let p = TPacketGCPhase::from_bytes(&exp).unwrap();
        assert_eq!(p.header, 0xFD);
        assert_eq!(p.phase, phase::HANDSHAKE);
        assert_eq!(p.to_bytes(), exp);

        let exp_auth: [u8; 2] = [0xFD, 0x0A]; // PHASE_AUTH
        let p2 = TPacketGCPhase::from_bytes(&exp_auth).unwrap();
        assert_eq!(p2.phase, phase::AUTH);
        assert_eq!(p2.to_bytes(), exp_auth);
    }

    #[test]
    fn golden_gc_auth_success() {
        // Layout: 0x96 | dwLoginKey 0xDEADBEEF LE | bResult
        let exp: [u8; 6] = [0x96, 0xEF, 0xBE, 0xAD, 0xDE, 0x01];
        let p = TPacketGCAuthSuccess::from_bytes(&exp).unwrap();
        assert_eq!(p.header, 150);
        assert_eq!(p.dw_login_key, 0xDEAD_BEEF);
        assert_eq!(p.b_result, 1);
        assert_eq!(p.to_bytes(), exp);
    }

    #[test]
    fn golden_gc_login_failure() {
        // Layout: 0x07 | szStatus[9] = "WRONGPWD\0"
        let exp: [u8; 10] = [0x07, b'W', b'R', b'O', b'N', b'G', b'P', b'W', b'D', 0x00];
        let p = TPacketGCLoginFailure::from_bytes(&exp).unwrap();
        assert_eq!(p.header, 7);
        assert_eq!(p.status(), "WRONGPWD");
        assert_eq!(p.to_bytes(), exp);
    }

    #[test]
    fn golden_gc_login_success_449b() {
        // Construcción manual desde el layout REAL (packed 71 B):
        //   header @0 (0x20)
        //   players[i] @ 1 + i*71 — TSimplePlayer packed:
        //     dwID@0 szName@4 byJob@29 byLevel@30 dwPlayMinutes@31
        //     byST@35 byHT@36 byDX@37 byIQ@38 wMainPart@39 bChangeName@43
        //     wHairPart@44 wAccePart@48 bDummy@52 x@56 y@60 lAddr@64 wPort@68 skill@70
        //   guild_id[i] @ 356 + i*4
        //   guild_name[i][13] @ 376 + i*13
        //   handle @ 441, random_key @ 445 — total 449.
        let mut exp = [0u8; 449];
        exp[0] = 0x20;

        // player 0 @ 1
        let p0 = 1;
        exp[p0 + 0..p0 + 4].copy_from_slice(&0x1122_3344u32.to_le_bytes()); // dwID
        exp[p0 + 4..p0 + 4 + 8].copy_from_slice(b"Hercules"); // szName (resto a cero)
        exp[p0 + 29] = 0; // byJob
        exp[p0 + 30] = 42; // byLevel
        exp[p0 + 31..p0 + 35].copy_from_slice(&3600u32.to_le_bytes()); // dwPlayMinutes
        exp[p0 + 35] = 90; // byST
        exp[p0 + 36] = 80; // byHT
        exp[p0 + 37] = 70; // byDX
        exp[p0 + 38] = 60; // byIQ
        exp[p0 + 39..p0 + 43].copy_from_slice(&0xAABB_CCDDu32.to_le_bytes()); // wMainPart
        exp[p0 + 43] = 0; // bChangeName
        exp[p0 + 44..p0 + 48].copy_from_slice(&0x1122_3344u32.to_le_bytes()); // wHairPart
        exp[p0 + 48..p0 + 52].copy_from_slice(&0x5566_7788u32.to_le_bytes()); // wAccePart
        exp[p0 + 52..p0 + 56].fill(0xAB); // bDummy[4]
        exp[p0 + 56..p0 + 60].copy_from_slice(&969_600i32.to_le_bytes()); // x (unidades)
        exp[p0 + 60..p0 + 64].copy_from_slice(&278_400i32.to_le_bytes()); // y
        exp[p0 + 64..p0 + 68].copy_from_slice(&0x7F00_0001u32.to_le_bytes()); // lAddr (inet)
        exp[p0 + 68..p0 + 70].copy_from_slice(&30_003u16.to_le_bytes()); // wPort
        exp[p0 + 70] = 1; // skill_group

        // player 1 @ 72 — solo dwID + szName
        let p1 = 1 + 71;
        exp[p1..p1 + 4].copy_from_slice(&2u32.to_le_bytes());
        exp[p1 + 4..p1 + 4 + 5].copy_from_slice(b"Ninja");

        // player 4 @ 285
        let p4 = 1 + 4 * 71;
        exp[p4..p4 + 4].copy_from_slice(&99u32.to_le_bytes());

        // guild_id @ 356
        exp[356..360].copy_from_slice(&500u32.to_le_bytes());
        exp[360..364].copy_from_slice(&501u32.to_le_bytes());
        // guild_name @ 376
        exp[376..376 + 6].copy_from_slice(b"GuildA");
        exp[376 + 4 * 13..376 + 4 * 13 + 6].copy_from_slice(b"GuildE");
        // handle @ 441, random_key @ 445
        exp[441..445].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        exp[445..449].copy_from_slice(&0xCAFE_BABEu32.to_le_bytes());

        let p = TPacketGCLoginSuccess::from_bytes(&exp).unwrap();
        // header + offsets de los bloques
        assert_eq!(p.header, 0x20);
        assert_eq!(p.players[0].dw_id, 0x1122_3344);
        assert_eq!(p.players[0].name(), "Hercules");
        assert_eq!(p.players[0].by_level, 42);
        assert_eq!(p.players[0].dw_play_minutes, 3600);
        assert_eq!(
            (
                p.players[0].by_st,
                p.players[0].by_ht,
                p.players[0].by_dx,
                p.players[0].by_iq
            ),
            (90, 80, 70, 60)
        );
        assert_eq!(p.players[0].w_main_part, 0xAABB_CCDD);
        assert_eq!(p.players[0].w_hair_part, 0x1122_3344);
        assert_eq!(p.players[0].w_acce_part, 0x5566_7788);
        assert_eq!(p.players[0].b_dummy, [0xAB; 4]);
        assert_eq!((p.players[0].x, p.players[0].y), (969_600, 278_400));
        assert_eq!(p.players[0].l_addr, 0x7F00_0001);
        assert_eq!(p.players[0].w_port, 30_003);
        assert_eq!(p.players[0].skill_group, 1);
        assert_eq!(p.players[1].dw_id, 2);
        assert_eq!(p.players[1].name(), "Ninja");
        assert_eq!(p.players[4].dw_id, 99);
        assert_eq!(p.guild_id, [500, 501, 0, 0, 0]);
        assert_eq!(p.guild_name[0], from_cstr::<13>("GuildA"));
        assert_eq!(p.guild_name[4], from_cstr::<13>("GuildE"));
        assert_eq!(p.handle, 0xDEAD_BEEF);
        assert_eq!(p.random_key, 0xCAFE_BABE);
        // serialize → bytes idénticos
        assert_eq!(p.to_bytes(), exp);
    }

    #[test]
    fn golden_gc_character_add() {
        // Layout: 0x01 | dwVID 0x1000 LE | angle 1.5f32 LE | x=1000 y=-2000 z=300
        // | bType=2 | wRaceNum=101 | bMovingSpeed=80 | bAttackSpeed=90 | bStateFlag=0
        // | dwAffectFlag=[1,2] LE
        let mut exp = [0u8; 37];
        exp[0] = 0x01;
        exp[1..5].copy_from_slice(&0x1000u32.to_le_bytes());
        exp[5..9].copy_from_slice(&1.5f32.to_le_bytes()); // 0x3FC00000
        exp[9..13].copy_from_slice(&1000i32.to_le_bytes());
        exp[13..17].copy_from_slice(&(-2000i32).to_le_bytes());
        exp[17..21].copy_from_slice(&300i32.to_le_bytes());
        exp[21] = 2;
        exp[22..26].copy_from_slice(&101u32.to_le_bytes());
        exp[26] = 80;
        exp[27] = 90;
        exp[28] = 0;
        exp[29..33].copy_from_slice(&1u32.to_le_bytes());
        exp[33..37].copy_from_slice(&2u32.to_le_bytes());

        let p = TPacketGCCharacterAdd::from_bytes(&exp).unwrap();
        assert_eq!(p.header, 0x01);
        assert_eq!(p.dw_vid, 0x1000);
        assert_eq!(p.angle, 1.5);
        assert_eq!((p.x, p.y, p.z), (1000, -2000, 300));
        assert_eq!(p.b_type, 2);
        assert_eq!(p.w_race_num, 101);
        assert_eq!(
            (p.b_moving_speed, p.b_attack_speed, p.b_state_flag),
            (80, 90, 0)
        );
        assert_eq!(p.dw_affect_flag, [1, 2]);
        assert_eq!(p.to_bytes(), exp);
    }

    #[test]
    fn golden_gc_char_additional_info_70b() {
        // Layout (spec §3 + packet.h:891, packed, QUIVER ON):
        //   header(0x88=136) @0 | dwVID @1 | name[25] @5 | awPart[5] @30
        //   bEmpire @50 | dwGuildID @51 | dwLevel @55 | sAlignment(i16) @59
        //   bPKMode @61 | dwMountVnum @62 | dwArrow @66 — total 70.
        // Verificación de la suma: 1+4+25+20+1+4+4+2+1+4+4 = 70.
        let mut exp = [0u8; 70];
        exp[0] = 0x88;
        exp[1..5].copy_from_slice(&0x1234u32.to_le_bytes()); // dwVID
        exp[5..5 + 10].copy_from_slice(b"NPC_Farmer"); // name (resto a cero)
                                                       // awPart[5] @30..50 (ARMOR, WEAPON, HEAD, HAIR, ACCE)
        exp[30..34].copy_from_slice(&0x1001u32.to_le_bytes());
        exp[34..38].copy_from_slice(&0x1002u32.to_le_bytes());
        exp[38..42].copy_from_slice(&0x1003u32.to_le_bytes());
        exp[42..46].copy_from_slice(&0x1004u32.to_le_bytes());
        exp[46..50].copy_from_slice(&0x1005u32.to_le_bytes());
        exp[50] = 1; // bEmpire
        exp[51..55].copy_from_slice(&500u32.to_le_bytes()); // dwGuildID
        exp[55..59].copy_from_slice(&42u32.to_le_bytes()); // dwLevel
        exp[59..61].copy_from_slice(&100i16.to_le_bytes()); // sAlignment (0x0064 LE)
        exp[61] = 0; // bPKMode
        exp[62..66].copy_from_slice(&0x2000u32.to_le_bytes()); // dwMountVnum
        exp[66..70].copy_from_slice(&1234u32.to_le_bytes()); // dwArrow

        let p = TPacketGCCharacterAdditionalInfo::from_bytes(&exp).unwrap();
        assert_eq!(p.header, 136);
        assert_eq!(p.dw_vid, 0x1234);
        assert_eq!(p.name(), "NPC_Farmer");
        assert_eq!(p.aw_part, [0x1001, 0x1002, 0x1003, 0x1004, 0x1005]);
        assert_eq!(p.b_empire, 1);
        assert_eq!(p.dw_guild_id, 500);
        assert_eq!(p.dw_level, 42);
        assert_eq!(p.s_alignment, 100);
        assert_eq!(p.b_pk_mode, 0);
        assert_eq!(p.dw_mount_vnum, 0x2000);
        assert_eq!(p.dw_arrow, 1234);
        // serialize → bytes idénticos byte a byte
        assert_eq!(p.to_bytes(), exp);
    }

    // ------------------------------------------------------------------
    // Parseo seguro: longitudes incorrectas → Err, sin panics
    // ------------------------------------------------------------------

    #[test]
    fn bad_lengths_are_errors() {
        for bad in [&[0u8; 0][..], &[0u8; 12][..], &[0u8; 14][..]] {
            assert!(matches!(
                TPacketCGHandshake::from_bytes(bad),
                Err(ProtocolError::BadLength { .. })
            ));
        }
        assert!(TPacketCGLogin::from_bytes(&[0u8; 48]).is_err());
        assert!(TPacketCGLogin2::from_bytes(&[0u8; 51]).is_err());
        // Login3: SOLO 65 y 68 son válidos.
        assert!(TPacketCGLogin3::from_bytes(&[0u8; 64]).is_err());
        assert!(TPacketCGLogin3::from_bytes(&[0u8; 66]).is_err());
        assert!(TPacketCGLogin3::from_bytes(&[0u8; 67]).is_err());
        assert!(TPacketCGLogin3::from_bytes(&[0u8; 69]).is_err());
        assert!(TPacketCGPlayerSelect::from_bytes(&[0u8; 1]).is_err());
        assert!(TPacketCGPlayerDelete::from_bytes(&[0u8; 9]).is_err());
        assert!(TPacketCGPlayerCreate::from_bytes(&[0u8; 33]).is_err());
        assert!(TPacketGCHandshake::from_bytes(&[0u8; 12]).is_err());
        assert!(TPacketGCLoginKey::from_bytes(&[0u8; 4]).is_err());
        assert!(TPacketGCPhase::from_bytes(&[0u8; 1]).is_err());
        assert!(TPacketGCAuthSuccess::from_bytes(&[0u8; 5]).is_err());
        assert!(TPacketGCLoginFailure::from_bytes(&[0u8; 9]).is_err());
        assert!(TPacketGCEmpire::from_bytes(&[0u8; 1]).is_err());
        assert!(TSimplePlayer::from_bytes(&[0u8; 70]).is_err());
        assert!(TSimplePlayer::from_bytes(&[0u8; 72]).is_err());
        assert!(TPacketGCLoginSuccess::from_bytes(&[0u8; 448]).is_err());
        assert!(
            TPacketGCLoginSuccess::from_bytes(&[0u8; 474]).is_err(),
            "spec dice 474, wire real 449"
        );
        assert!(TPacketGCCharacterAdd::from_bytes(&[0u8; 36]).is_err());
        assert!(TPacketGCCharacterAdditionalInfo::from_bytes(&[0u8; 69]).is_err());
    }

    #[test]
    fn from_cstr_truncates_and_nul_terminates() {
        // strlcpy semantics: 31-byte buffer, "x".repeat(40) → 30 bytes + NUL.
        let a = from_cstr::<31>(&"x".repeat(40));
        assert_eq!(a[30], 0);
        assert_eq!(cstr_bytes(&a).len(), 30);
        assert_eq!(from_cstr::<3>("es"), *b"es\0");
        assert_eq!(from_cstr::<3>("esp"), *b"es\0"); // truncado a 2 + NUL
                                                     // N=0: antes hacía overflow en `N - 1` (panic); ahora buffer vacío.
        assert_eq!(from_cstr::<0>("x"), [0u8; 0]);
        assert_eq!(from_cstr::<1>("abc"), [0u8; 1]); // 0 bytes + NUL implícito
    }
}
