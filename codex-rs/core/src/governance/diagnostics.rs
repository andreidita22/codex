use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceDiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceViolationKind {
    HierarchyBug,
    WorkerBug,
    InheritanceBug,
    RuntimeContaminationBug,
    RoleUnderInstantiation,
    AdapterOverlayBug,
    IllegalAmendment,
    PacketProjectionBug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceDiagnosticCode {
    MissingConstitutionPacket,
    ConstitutionIdentityChanged,
    MissingRolePacket,
    RoleIdentityChanged,
    MissingTaskCharterPacket,
    TaskCharterIdentityChanged,
    MissingTaskResidualPacket,
    TaskResidualIdentityChanged,
    MissingAdapterOverlayPacket,
    AdapterOverlayIdentityChanged,
    SpawnRuntimeFactStoreReused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GovernanceDiagnostic {
    pub code: GovernanceDiagnosticCode,
    pub kind: GovernanceViolationKind,
    pub severity: GovernanceDiagnosticSeverity,
    pub message: String,
}

impl GovernanceDiagnostic {
    pub fn error(
        code: GovernanceDiagnosticCode,
        kind: GovernanceViolationKind,
        message: String,
    ) -> Self {
        Self {
            code,
            kind,
            severity: GovernanceDiagnosticSeverity::Error,
            message,
        }
    }

    pub fn warning(
        code: GovernanceDiagnosticCode,
        kind: GovernanceViolationKind,
        message: String,
    ) -> Self {
        Self {
            code,
            kind,
            severity: GovernanceDiagnosticSeverity::Warning,
            message,
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self.severity, GovernanceDiagnosticSeverity::Error)
    }
}
