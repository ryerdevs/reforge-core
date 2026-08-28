use crate::account::pg_err;
use crate::pool::{Client, PgPool};
use crate::wal::{Batcher, Mutation, Param};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRow {
    pub id: i64,
    pub name: String,
    pub start_ts: i64,
    pub end_ts: i64,
    pub trigger_type: i16,
    pub trigger_value: i32,
}

const LIST_SQL: &str = "SELECT id, name, start_ts, end_ts, trigger_type, trigger_value FROM player.event ORDER BY id";
const INSERT_SQL: &str = "INSERT INTO player.event (name, start_ts, end_ts, trigger_type, trigger_value) VALUES ($1, $2, $3, $4, $5) RETURNING id";
const DELETE_SQL: &str = "DELETE FROM player.event WHERE id = $1";

pub struct EventRepo {
    pool: PgPool,
}
impl EventRepo {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
    async fn connect(&self) -> Result<Client, String> { self.pool.get().await.map_err(|e| format!("PG pool get: {e}")) }
    pub async fn list(&self) -> Result<Vec<EventRow>, String> {
        let c = self.connect().await?;
        let rows = c.query(LIST_SQL, &[]).await.map_err(|e| pg_err("EVENT_LIST", &e))?;
        rows.iter().map(|r| Ok(EventRow { id: r.try_get(0).map_err(|e| format!("col0 id:{e}"))?, name: r.try_get(1).map_err(|e| format!("col1 name:{e}"))?, start_ts: r.try_get(2).map_err(|e| format!("col2:{e}"))?, end_ts: r.try_get(3).map_err(|e| format!("col3:{e}"))?, trigger_type: r.try_get(4).map_err(|e| format!("col4:{e}"))?, trigger_value: r.try_get(5).map_err(|e| format!("col5:{e}"))? })).collect()
    }
    pub async fn create(&self, r: &EventRow) -> Result<i64, String> {
        let c = self.connect().await?;
        let row = c.query_one(INSERT_SQL, &[&r.name, &r.start_ts, &r.end_ts, &r.trigger_type, &r.trigger_value]).await.map_err(|e| pg_err("EVENT_CREATE", &e))?;
        row.try_get(0).map_err(|e| format!("EVENT_CREATE id:{e}"))
    }
    pub async fn delete(&self, id: i64) -> Result<u64, String> {
        let c = self.connect().await?;
        c.execute(DELETE_SQL, &[&id]).await.map_err(|e| pg_err("EVENT_DELETE", &e))
    }
    pub fn create_mutated(&self, b: &Batcher, r: &EventRow) { b.push(event_mutation(r)); }
}

pub(crate) fn event_mutation(r: &EventRow) -> Mutation {
    Mutation::new(INSERT_SQL, vec![Param::Text(r.name.clone()), Param::Int(r.start_ts), Param::Int(r.end_ts), Param::Int(i64::from(r.trigger_type)), Param::Int(i64::from(r.trigger_value))])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn list_sql_has_6_columns_ordered() {
        let cols: Vec<&str> = LIST_SQL.split_once(" FROM ").unwrap().0.trim_start_matches("SELECT ").split(',').map(|c| c.trim()).collect();
        assert_eq!(cols, ["id", "name", "start_ts", "end_ts", "trigger_type", "trigger_value"]);
        assert!(LIST_SQL.contains("FROM player.event ORDER BY id"));
    }
    #[test]
    fn insert_sql_is_id_lane_returning() {
        assert!(INSERT_SQL.contains("INSERT INTO player.event"));
        assert!(INSERT_SQL.contains("RETURNING id"), "id lane: RETURNING id");
        assert!(!INSERT_SQL.contains("ON CONFLICT"), "insert id lane sin upsert");
    }
    #[test]
    fn delete_sql_shape() { assert_eq!(DELETE_SQL, "DELETE FROM player.event WHERE id = $1"); }
    #[test]
    fn mutation_uses_shared_sql_and_params() {
        let r = EventRow { id: 0, name: "xmas".into(), start_ts: 1000, end_ts: 2000, trigger_type: 1, trigger_value: 2 };
        let m = event_mutation(&r);
        assert_eq!(m.sql, INSERT_SQL);
        assert_eq!(m.params, vec![Param::Text("xmas".into()), Param::Int(1000), Param::Int(2000), Param::Int(1), Param::Int(2)]);
        assert_eq!(m.id[6] >> 4, 7);
    }
}
