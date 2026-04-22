use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SchemaId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OperatorId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RegistryVersion(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrySourceFamily {
    Builtin,
    Project,
    UserApproved,
    SkillProvided,
    GovernanceProvided,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryConflictKind {
    DuplicateSchemaId,
    DuplicateOperatorId,
    AliasTargetConflict,
    InvalidSourceOverride,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryConflictResolution {
    RejectSnapshot,
    ShadowedByHigherPrecedence,
    IdenticalDuplicateIgnored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalOperator {
    Review,
    Audit,
    Verify,
    Judge,
    Compare,
    Implement,
    Debug,
    Summarize,
    Plan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredOperator {
    pub operator_id: OperatorId,
    pub canonical_operator: CanonicalOperator,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredSchema {
    pub schema_id: SchemaId,
    pub operator_id: OperatorId,
    pub source_family: RegistrySourceFamily,
    pub summary: String,
    pub trigger_phrases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AliasTarget {
    Schema { schema_id: SchemaId },
    Operator { operator_id: OperatorId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaAlias {
    pub alias: String,
    pub target: AliasTarget,
    pub source_family: RegistrySourceFamily,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistrySource {
    pub family: RegistrySourceFamily,
    pub operators: Vec<RegisteredOperator>,
    pub schemas: Vec<RegisteredSchema>,
    pub aliases: Vec<SchemaAlias>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaRegistrySnapshot {
    pub version: RegistryVersion,
    pub operators: Vec<RegisteredOperator>,
    pub schemas: Vec<RegisteredSchema>,
    pub aliases: Vec<SchemaAlias>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RegistrySnapshotError {
    #[error(
        "registry conflict ({kind:?}, resolution={resolution:?}) for key `{key}` between {existing_source:?} and {incoming_source:?}"
    )]
    Conflict {
        kind: RegistryConflictKind,
        resolution: RegistryConflictResolution,
        key: String,
        existing_source: RegistrySourceFamily,
        incoming_source: RegistrySourceFamily,
    },
    #[error("semantic broker v0 only supports builtin registry sources; found {0:?}")]
    UnsupportedSourceFamily(RegistrySourceFamily),
}

pub fn build_v0_registry_snapshot(
    version: RegistryVersion,
    sources: Vec<RegistrySource>,
) -> Result<SchemaRegistrySnapshot, RegistrySnapshotError> {
    let mut operators = BTreeMap::<OperatorId, (RegisteredOperator, RegistrySourceFamily)>::new();
    let mut schemas = BTreeMap::<SchemaId, (RegisteredSchema, RegistrySourceFamily)>::new();
    let mut aliases = BTreeMap::<String, (SchemaAlias, RegistrySourceFamily)>::new();

    for source in sources {
        if source.family != RegistrySourceFamily::Builtin {
            return Err(RegistrySnapshotError::UnsupportedSourceFamily(
                source.family,
            ));
        }

        for operator in source.operators {
            let key = operator.operator_id.clone();
            if let Some((existing, existing_source)) = operators.get(&key) {
                if existing != &operator {
                    return Err(RegistrySnapshotError::Conflict {
                        kind: RegistryConflictKind::DuplicateOperatorId,
                        resolution: RegistryConflictResolution::RejectSnapshot,
                        key: key.0,
                        existing_source: *existing_source,
                        incoming_source: source.family,
                    });
                }
            } else {
                operators.insert(key, (operator, source.family));
            }
        }

        for schema in source.schemas {
            let key = schema.schema_id.clone();
            if let Some((existing, existing_source)) = schemas.get(&key) {
                if existing != &schema {
                    return Err(RegistrySnapshotError::Conflict {
                        kind: RegistryConflictKind::DuplicateSchemaId,
                        resolution: RegistryConflictResolution::RejectSnapshot,
                        key: key.0,
                        existing_source: *existing_source,
                        incoming_source: source.family,
                    });
                }
            } else {
                schemas.insert(key, (schema, source.family));
            }
        }

        for alias in source.aliases {
            let key = alias.alias.clone();
            if let Some((existing, existing_source)) = aliases.get(&key) {
                if existing != &alias {
                    return Err(RegistrySnapshotError::Conflict {
                        kind: RegistryConflictKind::AliasTargetConflict,
                        resolution: RegistryConflictResolution::RejectSnapshot,
                        key,
                        existing_source: *existing_source,
                        incoming_source: source.family,
                    });
                }
            } else {
                aliases.insert(key, (alias, source.family));
            }
        }
    }

    Ok(SchemaRegistrySnapshot {
        version,
        operators: operators
            .into_values()
            .map(|(operator, _)| operator)
            .collect(),
        schemas: schemas.into_values().map(|(schema, _)| schema).collect(),
        aliases: aliases.into_values().map(|(alias, _)| alias).collect(),
    })
}

pub fn builtin_registry() -> Result<SchemaRegistrySnapshot, RegistrySnapshotError> {
    build_v0_registry_snapshot(
        RegistryVersion("semantic_broker.v0.builtin@1".to_string()),
        vec![RegistrySource {
            family: RegistrySourceFamily::Builtin,
            operators: vec![
                RegisteredOperator {
                    operator_id: OperatorId("review".to_string()),
                    canonical_operator: CanonicalOperator::Review,
                    description: "Review existing code or design for issues and risks.".to_string(),
                },
                RegisteredOperator {
                    operator_id: OperatorId("audit".to_string()),
                    canonical_operator: CanonicalOperator::Audit,
                    description: "Audit a system or change against a scoped checklist.".to_string(),
                },
                RegisteredOperator {
                    operator_id: OperatorId("verify".to_string()),
                    canonical_operator: CanonicalOperator::Verify,
                    description: "Verify whether behavior or a claim holds.".to_string(),
                },
                RegisteredOperator {
                    operator_id: OperatorId("compare".to_string()),
                    canonical_operator: CanonicalOperator::Compare,
                    description: "Compare alternatives, revisions, or states.".to_string(),
                },
                RegisteredOperator {
                    operator_id: OperatorId("implement".to_string()),
                    canonical_operator: CanonicalOperator::Implement,
                    description: "Implement a requested code or configuration change.".to_string(),
                },
                RegisteredOperator {
                    operator_id: OperatorId("debug".to_string()),
                    canonical_operator: CanonicalOperator::Debug,
                    description: "Diagnose and fix a bug or failing behavior.".to_string(),
                },
                RegisteredOperator {
                    operator_id: OperatorId("plan".to_string()),
                    canonical_operator: CanonicalOperator::Plan,
                    description: "Turn a target change into concrete implementation steps."
                        .to_string(),
                },
                RegisteredOperator {
                    operator_id: OperatorId("summarize".to_string()),
                    canonical_operator: CanonicalOperator::Summarize,
                    description: "Condense provided material into a concise summary.".to_string(),
                },
            ],
            schemas: vec![
                RegisteredSchema {
                    schema_id: SchemaId("workflow.review.code".to_string()),
                    operator_id: OperatorId("review".to_string()),
                    source_family: RegistrySourceFamily::Builtin,
                    summary: "Code review with findings-first output.".to_string(),
                    trigger_phrases: vec![
                        "review this".to_string(),
                        "review the".to_string(),
                        "code review".to_string(),
                    ],
                },
                RegisteredSchema {
                    schema_id: SchemaId("workflow.audit.system".to_string()),
                    operator_id: OperatorId("audit".to_string()),
                    source_family: RegistrySourceFamily::Builtin,
                    summary: "Audit focused on boundary, ownership, and risk findings.".to_string(),
                    trigger_phrases: vec![
                        "audit this".to_string(),
                        "audit the".to_string(),
                        "run an audit".to_string(),
                    ],
                },
                RegisteredSchema {
                    schema_id: SchemaId("workflow.verify.behavior".to_string()),
                    operator_id: OperatorId("verify".to_string()),
                    source_family: RegistrySourceFamily::Builtin,
                    summary: "Verify whether behavior, claim, or invariant holds.".to_string(),
                    trigger_phrases: vec![
                        "verify this".to_string(),
                        "check whether".to_string(),
                        "confirm that".to_string(),
                    ],
                },
                RegisteredSchema {
                    schema_id: SchemaId("workflow.compare.options".to_string()),
                    operator_id: OperatorId("compare".to_string()),
                    source_family: RegistrySourceFamily::Builtin,
                    summary: "Compare alternatives or revisions with explicit tradeoffs."
                        .to_string(),
                    trigger_phrases: vec![
                        "compare".to_string(),
                        "difference between".to_string(),
                        "versus".to_string(),
                    ],
                },
                RegisteredSchema {
                    schema_id: SchemaId("workflow.implement.change".to_string()),
                    operator_id: OperatorId("implement".to_string()),
                    source_family: RegistrySourceFamily::Builtin,
                    summary: "Implement a concrete change in the codebase.".to_string(),
                    trigger_phrases: vec![
                        "implement".to_string(),
                        "add support".to_string(),
                        "make this change".to_string(),
                    ],
                },
                RegisteredSchema {
                    schema_id: SchemaId("workflow.debug.failure".to_string()),
                    operator_id: OperatorId("debug".to_string()),
                    source_family: RegistrySourceFamily::Builtin,
                    summary: "Diagnose failing behavior and apply a fix.".to_string(),
                    trigger_phrases: vec![
                        "debug".to_string(),
                        "fix this".to_string(),
                        "why is".to_string(),
                    ],
                },
                RegisteredSchema {
                    schema_id: SchemaId("workflow.plan.execution".to_string()),
                    operator_id: OperatorId("plan".to_string()),
                    source_family: RegistrySourceFamily::Builtin,
                    summary: "Plan an implementation or migration sequence.".to_string(),
                    trigger_phrases: vec![
                        "plan this".to_string(),
                        "implementation plan".to_string(),
                        "what should we do".to_string(),
                    ],
                },
                RegisteredSchema {
                    schema_id: SchemaId("workflow.summarize.material".to_string()),
                    operator_id: OperatorId("summarize".to_string()),
                    source_family: RegistrySourceFamily::Builtin,
                    summary: "Summarize provided material without inventing new procedure."
                        .to_string(),
                    trigger_phrases: vec![
                        "summarize".to_string(),
                        "tl;dr".to_string(),
                        "give me the summary".to_string(),
                    ],
                },
            ],
            aliases: vec![
                SchemaAlias {
                    alias: "review".to_string(),
                    target: AliasTarget::Operator {
                        operator_id: OperatorId("review".to_string()),
                    },
                    source_family: RegistrySourceFamily::Builtin,
                },
                SchemaAlias {
                    alias: "audit".to_string(),
                    target: AliasTarget::Operator {
                        operator_id: OperatorId("audit".to_string()),
                    },
                    source_family: RegistrySourceFamily::Builtin,
                },
                SchemaAlias {
                    alias: "verify".to_string(),
                    target: AliasTarget::Operator {
                        operator_id: OperatorId("verify".to_string()),
                    },
                    source_family: RegistrySourceFamily::Builtin,
                },
            ],
        }],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn builtin_source() -> RegistrySource {
        RegistrySource {
            family: RegistrySourceFamily::Builtin,
            operators: vec![RegisteredOperator {
                operator_id: OperatorId("review".to_string()),
                canonical_operator: CanonicalOperator::Review,
                description: "desc".to_string(),
            }],
            schemas: vec![RegisteredSchema {
                schema_id: SchemaId("workflow.review.code".to_string()),
                operator_id: OperatorId("review".to_string()),
                source_family: RegistrySourceFamily::Builtin,
                summary: "summary".to_string(),
                trigger_phrases: vec!["review".to_string()],
            }],
            aliases: vec![SchemaAlias {
                alias: "review".to_string(),
                target: AliasTarget::Schema {
                    schema_id: SchemaId("workflow.review.code".to_string()),
                },
                source_family: RegistrySourceFamily::Builtin,
            }],
        }
    }

    #[test]
    fn rejects_duplicate_schema_id_with_different_content() {
        let mut source = builtin_source();
        source.schemas.push(RegisteredSchema {
            schema_id: SchemaId("workflow.review.code".to_string()),
            operator_id: OperatorId("review".to_string()),
            source_family: RegistrySourceFamily::Builtin,
            summary: "different".to_string(),
            trigger_phrases: vec!["review".to_string()],
        });

        let err = build_v0_registry_snapshot(RegistryVersion("v0".to_string()), vec![source])
            .expect_err("duplicate schema should fail");
        assert_eq!(
            err,
            RegistrySnapshotError::Conflict {
                kind: RegistryConflictKind::DuplicateSchemaId,
                resolution: RegistryConflictResolution::RejectSnapshot,
                key: "workflow.review.code".to_string(),
                existing_source: RegistrySourceFamily::Builtin,
                incoming_source: RegistrySourceFamily::Builtin,
            }
        );
    }

    #[test]
    fn ignores_exact_duplicate_schema_content() {
        let mut source = builtin_source();
        source.schemas.push(source.schemas[0].clone());

        let snapshot = build_v0_registry_snapshot(RegistryVersion("v0".to_string()), vec![source])
            .expect("identical duplicate should be ignored");
        assert_eq!(snapshot.schemas.len(), 1);
    }

    #[test]
    fn rejects_alias_target_conflicts() {
        let mut source = builtin_source();
        source.aliases.push(SchemaAlias {
            alias: "review".to_string(),
            target: AliasTarget::Operator {
                operator_id: OperatorId("review".to_string()),
            },
            source_family: RegistrySourceFamily::Builtin,
        });

        let err = build_v0_registry_snapshot(RegistryVersion("v0".to_string()), vec![source])
            .expect_err("conflicting alias should fail");
        assert_eq!(
            err,
            RegistrySnapshotError::Conflict {
                kind: RegistryConflictKind::AliasTargetConflict,
                resolution: RegistryConflictResolution::RejectSnapshot,
                key: "review".to_string(),
                existing_source: RegistrySourceFamily::Builtin,
                incoming_source: RegistrySourceFamily::Builtin,
            }
        );
    }
}
