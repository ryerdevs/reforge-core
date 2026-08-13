//! AST of the quest DSL (spec §2, §6, §7, §10).

/// A parsed quest file: imports + blocks + quests (families and instances).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestFile {
    pub imports: Vec<String>,
    pub blocks: Vec<Block>,
    pub quests: Vec<Quest>,
}

/// A reusable block (`block name(param: type, ...) -> actions...`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
}

/// A quest: either a concrete quest or a parameterized family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Quest {
    /// `quest <name>` — concrete.
    Concrete(QuestDef),
    /// `quest <name> family (<params>)` — parameterized template (spec §6).
    Family { name: String, params: Vec<Param>, states: Vec<State> },
    /// `quest <name> = <base>(<param>: <value>, ...)` — instance (spec §6).
    Instance(InstanceDef),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestDef {
    pub name: String,
    pub states: Vec<State>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceDef {
    pub name: String,
    pub base: String,
    pub args: Vec<(String, Value)>,
}

/// `state <name>` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub name: String,
    pub events: Vec<Event>,
}

/// `on <trigger>[, <trigger>...] [with <expr>]` + body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub triggers: Vec<Trigger>,
    pub condition: Option<Expr>,
    pub body: Vec<Stmt>,
}

/// A trigger (spec §3): `login`, `levelup`, `20084.chat`, `601.kill`, ...
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trigger {
    pub kind: TriggerKind,
}

/// Target de un trigger `<vnum>.chat/kill/use` — número fijo o parámetro de
/// familia `(mob)` (spec §6: `on (mob).kill`), resuelto en la expansión.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerTarget {
    Num(u32),
    Param(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerKind {
    Login,
    LevelUp,
    Letter,
    Button,
    Info,
    Enter,
    Logout,
    Timer,
    Chat { target: TriggerTarget },
    Kill { target: TriggerTarget },
    Use { target: TriggerTarget },
    TargetClick,
    /// `arena.*`, `oxevent.*`, `d.*`, `wedding.*` → Rust modules (spec §8).
    Rust(String),
}

/// Statement inside an event body: action, capture, branch or block use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    /// `-> action(args)` with optional `as capture`.
    Action { action: Action, capture: Option<String> },
    /// `if expr` / `else` — 1 level + else (spec §10, decision §11.2).
    Branch(Branch),
    /// `use block(args)` (spec §7).
    Use { name: String, args: Vec<Value> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub condition: Option<Expr>, // None = else
    pub body: Vec<Stmt>,
}

/// An action with its typed args (spec §5 catalog).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub name: ActionName,
    pub args: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionName {
    SayTitle,
    Say,
    SayReward,
    SayItemVnum,
    SendLetter,
    ClearLetter,
    Wait,
    SetState,
    SetQuestState,
    SetQf,
    GiveItem2,
    RemoveItem,
    TargetVid,
    TargetDelete,
    Warp,
    Notice,
    NoticeMultiline,
    AffectAdd,
    AffectRemove,
    Select,
    InputNumber,
    Return,
}

/// A condition expression (spec §4): comparisons, arithmetic, functions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// `(a) between b, c` — native range syntax (decision §11.1).
    Between(Box<Expr>, Box<Expr>, Box<Expr>),
    Compare(Box<Expr>, CmpOp, Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Value(Value),
    /// Bare identifier in a condition — a capture from `as <name>` (spec §10:
    /// `if choice == 1`).
    Capture(String),
    /// Function call: `pc.level`, `count_item(30006)`, `number(1, 100)`, ...
    Func(FuncName, Vec<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FuncName {
    PcLevel,
    CountItem,
    GetQf,
    Number,
    GetTime,
    GetMapIndex,
    GetGmLevel,
    PetIsSummon,
    IsTestServer,
}

/// A literal value: number, string or `@key` locale key (spec §6), family
/// parameter reference `(name)`, or a full expression — `set_qf` values and
/// `affect_add` durations are evaluated by the runtime
/// (`set_qf(duration, get_time() + 60 * 60 * 22)`,
/// `affect_add(apply.MOV_SPEED, 10, 60 * 60 * 24 * 365 * 60)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Num(i64),
    Str(String),
    /// `@key` — locale key (family key index).
    Key(String),
    /// `(name)` — family parameter reference, resolved at expansion.
    Param(String),
    /// Full condition expression as an argument (runtime-evaluated).
    Expr(Box<Expr>),
}

/// A block/quest parameter. Type is OPTIONAL: family params and block params
/// may be bare names (`quest X family (level, mob)` — spec §6) or typed
/// (`block npc_target(npc: vnum, key: key)` — spec §7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: Option<ParamType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamType {
    Vnum,
    Level,
    Key,
    Str,
}
