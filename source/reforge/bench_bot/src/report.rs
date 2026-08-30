//! Métricas del benchmark: el reporte por bot, los agregados y el renderizado
//! (tabla de texto + JSON — std-only, sin serde: formato fijo del harness).

/// Estado final de un bot (deriva del error de la sesión, si lo hubo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    /// El auth rechazó el LOGIN3 (bResult=0).
    AuthFailed,
    /// El canal respondió `GC_LOGIN_FAILURE` (p.ej. "ALREADY", "NOID").
    LoginFailed,
    /// La cuenta no tiene personaje en ningún slot.
    NoCharacter,
    /// Fallo tras el select (entry / entergame / fase game).
    WorldFailed,
    /// El server envió un header fuera de la tabla S→C (drift de protocolo).
    Desync,
    /// El server cerró la conexión.
    Disconnected,
    /// Timeout de una fase (auth/select/entry).
    Timeout,
}

impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::AuthFailed => "auth_fail",
            Status::LoginFailed => "login_fail",
            Status::NoCharacter => "no_char",
            Status::WorldFailed => "world_fail",
            Status::Desync => "desync",
            Status::Disconnected => "disconnected",
            Status::Timeout => "timeout",
        }
    }
}

/// Reporte por bot.
#[derive(Debug, Clone)]
pub struct BotReport {
    pub index: usize,
    pub login: String,
    pub status: Status,
    /// Nota adicional (p.ej. el status del `GC_LOGIN_FAILURE`, el header del
    /// desync o el mensaje del error).
    pub note: String,
    /// Connect→`GC_AUTH_SUCCESS`.
    pub auth_ms: Option<u64>,
    /// Connect canal→449 B (`GC_LOGIN_SUCCESS_NEWSLOT`).
    pub channel_login_ms: Option<u64>,
    /// Select enviado→`GC_PHASE(GAME)` (la entrada al mundo).
    pub select_ms: Option<u64>,
    /// Tiempo total connect auth→`GC_PHASE(GAME)` (la métrica "login→world").
    pub world_ms: Option<u64>,
    /// Tiempo dentro del mundo (hasta el cierre o el fin de la duración).
    pub alive_ms: u64,
    pub rx_packets: u64,
    pub rx_bytes: u64,
    pub tx_packets: u64,
    pub tx_bytes: u64,
    pub moves: u64,
    pub pings: u64,
    /// `GC_CHARACTER_ADD` recibidos en fase game (mobs visibles — la dimensión
    /// de densidad del benchmark).
    pub spawns: u64,
}

impl BotReport {
    pub fn failed(&self) -> bool {
        self.status != Status::Ok
    }
}

/// Estadística de una serie de tiempos (ms).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MsStats {
    pub min: u64,
    pub median: u64,
    pub p95: u64,
    pub max: u64,
}

/// `min/median/p95/max` de una serie. `None` si la serie está vacía.
/// p95 = el valor en el percentil 95 (índice `ceil(0.95*n)-1`).
pub fn ms_stats(values: &[u64]) -> Option<MsStats> {
    if values.is_empty() {
        return None;
    }
    let mut v: Vec<u64> = values.to_vec();
    v.sort_unstable();
    let n = v.len();
    let median = if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2
    };
    let p95_idx = (n as f64 * 0.95).ceil() as usize - 1;
    Some(MsStats {
        min: v[0],
        median,
        p95: v[p95_idx.min(n - 1)],
        max: v[n - 1],
    })
}

/// Agregados del run.
#[derive(Debug, Clone, Default)]
pub struct Summary {
    pub ok: usize,
    pub failed: usize,
    /// ms_stats de `world_ms` de los bots OK (login→world).
    pub world_ms: Option<MsStats>,
    pub auth_ms: Option<MsStats>,
    pub channel_login_ms: Option<MsStats>,
    pub select_ms: Option<MsStats>,
    pub total_rx_packets: u64,
    pub total_rx_bytes: u64,
    pub total_tx_packets: u64,
    pub total_tx_bytes: u64,
    pub total_moves: u64,
    pub total_spawns: u64,
}

pub fn summarize(reports: &[BotReport]) -> Summary {
    let mut s = Summary::default();
    let mut worlds = Vec::new();
    let mut auths = Vec::new();
    let mut logins = Vec::new();
    let mut selects = Vec::new();
    for r in reports {
        if r.failed() {
            s.failed += 1;
        } else {
            s.ok += 1;
        }
        if let Some(v) = r.world_ms {
            worlds.push(v);
        }
        if let Some(v) = r.auth_ms {
            auths.push(v);
        }
        if let Some(v) = r.channel_login_ms {
            logins.push(v);
        }
        if let Some(v) = r.select_ms {
            selects.push(v);
        }
        s.total_rx_packets += r.rx_packets;
        s.total_rx_bytes += r.rx_bytes;
        s.total_tx_packets += r.tx_packets;
        s.total_tx_bytes += r.tx_bytes;
        s.total_moves += r.moves;
        s.total_spawns += r.spawns;
    }
    s.world_ms = ms_stats(&worlds);
    s.auth_ms = ms_stats(&auths);
    s.channel_login_ms = ms_stats(&logins);
    s.select_ms = ms_stats(&selects);
    s
}

fn fmt_ms(v: Option<u64>) -> String {
    match v {
        Some(x) => x.to_string(),
        None => "-".into(),
    }
}

/// Tabla resumen por bot (una línea por bot).
pub fn render_table(reports: &[BotReport], summary: &Summary) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:>4}  {:<16} {:<13} {:>7} {:>7} {:>7} {:>8} {:>7} {:>6} {:>6} {:>5} {:>5}  {}\n",
        "#",
        "login",
        "status",
        "auth_ms",
        "login_ms",
        "sel_ms",
        "world_ms",
        "alive_s",
        "rx_pkts",
        "tx_pkts",
        "moves",
        "spawns",
        "note"
    ));
    for r in reports {
        out.push_str(&format!(
            "{:>4}  {:<16} {:<13} {:>7} {:>7} {:>7} {:>8} {:>7} {:>6} {:>6} {:>5} {:>5}  {}\n",
            r.index,
            r.login,
            r.status.label(),
            fmt_ms(r.auth_ms),
            fmt_ms(r.channel_login_ms),
            fmt_ms(r.select_ms),
            fmt_ms(r.world_ms),
            r.alive_ms / 1000,
            r.rx_packets,
            r.tx_packets,
            r.moves,
            r.spawns,
            r.note
        ));
    }
    let w = summary
        .world_ms
        .map(|s| format!("{}", s.median))
        .unwrap_or_else(|| "-".into());
    out.push_str(&format!(
        "ok {}/{} | world_ms (ok): min {:?} | median {} | p95 {:?} | max {:?} | rx {} pkt/{} B | tx {} pkt/{} B | moves {} | spawns {}\n",
        summary.ok,
        summary.ok + summary.failed,
        summary.world_ms.map(|s| s.min),
        w,
        summary.world_ms.map(|s| s.p95),
        summary.world_ms.map(|s| s.max),
        summary.total_rx_packets,
        summary.total_rx_bytes,
        summary.total_tx_packets,
        summary.total_tx_bytes,
        summary.total_moves,
        summary.total_spawns,
    ));
    out
}

/// Línea de resumen agregada (modo `--summary` — para leer la escalera sin
/// abrir archivos): ok/total + min/median/p95/max de auth/sel/world + tráfico.
pub fn render_summary_line(s: &Summary) -> String {
    let stats = |m: Option<MsStats>| -> String {
        match m {
            Some(v) => format!("{}/{}/{}/{}", v.min, v.median, v.p95, v.max),
            None => "-".into(),
        }
    };
    format!(
        "ok {}/{} | auth_ms {} | sel_ms {} | world_ms {} | rx {} pkt/{} B | tx {} pkt/{} B | moves {} | spawns {}",
        s.ok,
        s.ok + s.failed,
        stats(s.auth_ms),
        stats(s.select_ms),
        stats(s.world_ms),
        s.total_rx_packets,
        s.total_rx_bytes,
        s.total_tx_packets,
        s.total_tx_bytes,
        s.total_moves,
        s.total_spawns,
    )
}

/// Escapa una string para JSON (las únicas strings del reporte que vienen de
/// la red son las notas/status — se escapan por defensa).
pub fn esc_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// JSON del run (formato fijo del harness — sin serde).
pub fn render_json(reports: &[BotReport], summary: &Summary, meta: &[(&str, String)]) -> String {
    let mut out = String::new();
    out.push_str("{\n  \"meta\": {");
    for (i, (k, v)) in meta.iter().enumerate() {
        out.push_str(&format!(
            "{}{:?}: {:?}",
            if i > 0 { ", " } else { "" },
            k,
            v
        ));
    }
    out.push_str("},\n  \"summary\": {");
    out.push_str(&format!(
        "\"ok\": {}, \"failed\": {}, \"world_ms\": {}, \"auth_ms\": {}, \"channel_login_ms\": {}, \"select_ms\": {}, \
         \"rx_packets\": {}, \"rx_bytes\": {}, \"tx_packets\": {}, \"tx_bytes\": {}, \"moves\": {}, \"spawns\": {}",
        summary.ok,
        summary.failed,
        stats_json(summary.world_ms),
        stats_json(summary.auth_ms),
        stats_json(summary.channel_login_ms),
        stats_json(summary.select_ms),
        summary.total_rx_packets,
        summary.total_rx_bytes,
        summary.total_tx_packets,
        summary.total_tx_bytes,
        summary.total_moves,
        summary.total_spawns,
    ));
    out.push_str("},\n  \"bots\": [");
    for (i, r) in reports.iter().enumerate() {
        out.push_str(if i > 0 { ",\n    " } else { "\n    " });
        out.push_str(&format!(
            "{{\"index\": {}, \"login\": {:?}, \"status\": {:?}, \"note\": {:?}, \
             \"auth_ms\": {}, \"channel_login_ms\": {}, \"select_ms\": {}, \"world_ms\": {}, \
             \"alive_ms\": {}, \"rx_packets\": {}, \"rx_bytes\": {}, \"tx_packets\": {}, \
             \"tx_bytes\": {}, \"moves\": {}, \"pings\": {}, \"spawns\": {}}}",
            r.index,
            esc_json(&r.login),
            r.status.label(),
            esc_json(&r.note),
            json_opt(r.auth_ms),
            json_opt(r.channel_login_ms),
            json_opt(r.select_ms),
            json_opt(r.world_ms),
            r.alive_ms,
            r.rx_packets,
            r.rx_bytes,
            r.tx_packets,
            r.tx_bytes,
            r.moves,
            r.pings,
            r.spawns,
        ));
    }
    out.push_str("\n  ]\n}\n");
    out
}

fn json_opt(v: Option<u64>) -> String {
    match v {
        Some(x) => x.to_string(),
        None => "null".into(),
    }
}

fn stats_json(s: Option<MsStats>) -> String {
    match s {
        Some(v) => format!(
            "{{\"min\": {}, \"median\": {}, \"p95\": {}, \"max\": {}}}",
            v.min, v.median, v.p95, v.max
        ),
        None => "null".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(index: usize, status: Status, world_ms: Option<u64>, note: &str) -> BotReport {
        BotReport {
            index,
            login: format!("bench_{index}"),
            status,
            note: note.into(),
            auth_ms: Some(10 + index as u64),
            channel_login_ms: Some(20 + index as u64),
            select_ms: Some(30 + index as u64),
            world_ms,
            alive_ms: 5_000,
            rx_packets: 100,
            rx_bytes: 1_000,
            tx_packets: 50,
            tx_bytes: 500,
            moves: 4,
            pings: 1,
            spawns: 11,
        }
    }

    #[test]
    fn ms_stats_median_and_p95() {
        // par → mediana = media de los dos centrales
        let s = ms_stats(&[10, 20, 30, 40]).unwrap();
        assert_eq!(s.min, 10);
        assert_eq!(s.median, 25);
        assert_eq!(s.max, 40);
        // impar → central
        let s = ms_stats(&[10, 20, 30]).unwrap();
        assert_eq!(s.median, 20);
        // p95 de 20 valores = índice ceil(19)-1 = 18 (el 19º)
        let v: Vec<u64> = (0..20).collect();
        let s = ms_stats(&v).unwrap();
        assert_eq!(s.p95, 18);
        assert_eq!(s.max, 19);
        // vacío → None
        assert!(ms_stats(&[]).is_none());
    }

    #[test]
    fn summarize_counts_and_aggregates() {
        let reps = vec![
            report(0, Status::Ok, Some(50), ""),
            report(1, Status::Ok, Some(70), ""),
            report(2, Status::LoginFailed, None, "ALREADY"),
        ];
        let s = summarize(&reps);
        assert_eq!(s.ok, 2);
        assert_eq!(s.failed, 1);
        let w = s.world_ms.unwrap();
        assert_eq!((w.min, w.median, w.max), (50, 60, 70));
        assert_eq!(s.total_moves, 12);
    }

    #[test]
    fn json_escapes_quotes_and_controls() {
        assert_eq!(esc_json("a\"b"), "a\\\"b");
        assert_eq!(esc_json("a\\b"), "a\\\\b");
        assert_eq!(esc_json("a\nb"), "a\\nb");
        assert_eq!(esc_json("noop"), "noop");
    }

    #[test]
    fn json_output_is_parseable_shape() {
        let reps = vec![
            report(0, Status::Ok, Some(50), ""),
            report(1, Status::Desync, None, "0x99"),
        ];
        let s = summarize(&reps);
        let json = render_json(
            &reps,
            &s,
            &[("bots", "2".into()), ("duration_s", "5".into())],
        );
        // forma: clave/valor presentes y escapados
        assert!(json.starts_with("{\n  \"meta\""));
        assert!(json.contains("\"status\": \"ok\""));
        assert!(json.contains("\"status\": \"desync\""));
        assert!(json.contains("\"note\": \"0x99\""));
        assert!(json.contains("\"ok\": 1, \"failed\": 1"));
        assert!(json.contains("\"median\": 50"));
        assert!(json.ends_with("\n}\n"));
    }

    #[test]
    fn table_renders_statuses_and_summary_line() {
        let reps = vec![report(0, Status::Ok, Some(50), "")];
        let s = summarize(&reps);
        let table = render_table(&reps, &s);
        assert!(table.contains("bench_0"));
        assert!(table.contains("ok"));
        assert!(table.contains("ok 1/1"));
        assert!(table.contains("median 50"));
    }

    #[test]
    fn summary_line_renders_per_phase_stats() {
        let reps = vec![
            report(0, Status::Ok, Some(50), ""),
            report(1, Status::Ok, Some(70), ""),
            report(2, Status::LoginFailed, None, "ALREADY"),
        ];
        let s = summarize(&reps);
        let line = render_summary_line(&s);
        assert!(line.contains("ok 2/3"), "línea: {line}");
        // auth_ms = 10, 11, 12 → min/median/p95/max = 10/11/12/12.
        assert!(line.contains("auth_ms 10/11/12/12"), "línea: {line}");
        // sel_ms = 30, 31, 32 → 30/31/32/32; world_ms solo de los OK: 50, 70.
        assert!(line.contains("sel_ms 30/31/32/32"), "línea: {line}");
        assert!(line.contains("world_ms 50/60/70/70"), "línea: {line}");
        assert!(line.contains("moves 12"), "línea: {line}");
        assert!(line.contains("spawns 33"), "línea: {line}");
    }

    #[test]
    fn summary_line_handles_no_data() {
        let s = Summary::default();
        let line = render_summary_line(&s);
        assert!(line.contains("ok 0/0"), "línea: {line}");
        assert!(line.contains("world_ms -"), "sin datos → '-': {line}");
    }
}
