use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NormativeForce {
    Hard,
    Default,
    Advisory,
    Factual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PacketIdentity {
    pub packet_id: String,
    pub content_hash: String,
    pub schema: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GovernanceClause {
    pub force: NormativeForce,
    pub subject: String,
    pub instruction: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeFact {
    pub key: String,
    pub value: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConstitutionPacket {
    pub identity: PacketIdentity,
    pub clauses: Vec<GovernanceClause>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RolePacket {
    pub identity: PacketIdentity,
    pub role_name: String,
    pub posture: Vec<GovernanceClause>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TaskCharterPacket {
    pub identity: PacketIdentity,
    pub assignment_id: String,
    pub ordered_obligations: Vec<GovernanceClause>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TaskResidualPacket {
    pub identity: PacketIdentity,
    pub open_obligations: Vec<GovernanceClause>,
    pub blockers: Vec<String>,
    pub accepted_decisions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeFactStore {
    pub identity: PacketIdentity,
    pub facts: Vec<RuntimeFact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AdapterOverlayPacket {
    pub identity: PacketIdentity,
    pub adapter_id: String,
    pub clauses: Vec<GovernanceClause>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PacketLayer {
    Constitution,
    Role,
    TaskCharter,
    TaskResidual,
    RuntimeFactStore,
    AdapterOverlay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "packet_type", rename_all = "snake_case")]
pub enum GovernancePacket {
    Constitution(ConstitutionPacket),
    Role(RolePacket),
    TaskCharter(TaskCharterPacket),
    TaskResidual(TaskResidualPacket),
    RuntimeFactStore(RuntimeFactStore),
    AdapterOverlay(AdapterOverlayPacket),
}

impl GovernancePacket {
    pub fn identity(&self) -> &PacketIdentity {
        match self {
            Self::Constitution(packet) => &packet.identity,
            Self::Role(packet) => &packet.identity,
            Self::TaskCharter(packet) => &packet.identity,
            Self::TaskResidual(packet) => &packet.identity,
            Self::RuntimeFactStore(packet) => &packet.identity,
            Self::AdapterOverlay(packet) => &packet.identity,
        }
    }

    pub fn layer(&self) -> PacketLayer {
        match self {
            Self::Constitution(_) => PacketLayer::Constitution,
            Self::Role(_) => PacketLayer::Role,
            Self::TaskCharter(_) => PacketLayer::TaskCharter,
            Self::TaskResidual(_) => PacketLayer::TaskResidual,
            Self::RuntimeFactStore(_) => PacketLayer::RuntimeFactStore,
            Self::AdapterOverlay(_) => PacketLayer::AdapterOverlay,
        }
    }
}
