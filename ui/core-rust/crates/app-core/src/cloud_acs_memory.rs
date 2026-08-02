// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use product_contracts::{
    AcsCompareAndSwapRootRequest, AcsCompareAndSwapRootResponse, AcsCreateObjectOutcome,
    AcsCreateObjectRequest, AcsEncryptedValue, AcsEncryptedValueKind, AcsErrorCode,
    AcsErrorResponse, AcsObjectSnapshot, AcsRootSnapshot, AcsSseEvent, ACS_CONTRACT_ID,
    ACS_FIXED_ROOT_ID,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AcsMemoryDelivery<T> {
    Delivered(T),
    LostAfterCommit,
}

#[derive(Debug, Default)]
struct AcsMemoryAccount {
    objects: BTreeMap<String, AcsObjectSnapshot>,
    root: Option<AcsRootSnapshot>,
    next_sequence: u64,
    events: Vec<AcsSseEvent>,
}

#[derive(Debug, Default)]
pub(crate) struct InMemoryAcsProvider {
    accounts: BTreeMap<String, AcsMemoryAccount>,
    lose_next_root_response: bool,
}

impl InMemoryAcsProvider {
    pub(crate) fn create_account(&mut self, account_locator: &str) -> Result<(), AcsErrorResponse> {
        if account_locator.is_empty() {
            return Err(error(AcsErrorCode::InvalidRequest, "empty account locator"));
        }
        self.accounts
            .entry(account_locator.to_string())
            .or_default();
        Ok(())
    }

    pub(crate) fn create_object(
        &mut self,
        account_locator: &str,
        request: AcsCreateObjectRequest,
        now_epoch_ms: i64,
    ) -> Result<AcsCreateObjectOutcome, AcsErrorResponse> {
        require_contract(&request.contract_id)?;
        if request.object_id.is_empty() {
            return Err(error(AcsErrorCode::InvalidRequest, "empty object ID"));
        }
        request
            .value
            .validate()
            .map_err(|detail| error(AcsErrorCode::InvalidRequest, detail))?;
        let account = self.account_mut(account_locator)?;
        require_children(account, &request.value)?;
        if let Some(existing) = account.objects.get(&request.object_id) {
            return if existing.value == request.value {
                Ok(AcsCreateObjectOutcome::AlreadyExists)
            } else {
                Err(error(
                    AcsErrorCode::ObjectIdCollision,
                    "object ID already contains different authenticated data",
                ))
            };
        }
        account.objects.insert(
            request.object_id.clone(),
            AcsObjectSnapshot {
                object_id: request.object_id,
                value: request.value,
                created_at_epoch_ms: now_epoch_ms,
            },
        );
        Ok(AcsCreateObjectOutcome::Created)
    }

    pub(crate) fn read_object(
        &self,
        account_locator: &str,
        object_id: &str,
    ) -> Result<Option<AcsObjectSnapshot>, AcsErrorResponse> {
        Ok(self
            .account(account_locator)?
            .objects
            .get(object_id)
            .cloned())
    }

    pub(crate) fn root(
        &self,
        account_locator: &str,
    ) -> Result<Option<AcsRootSnapshot>, AcsErrorResponse> {
        Ok(self.account(account_locator)?.root.clone())
    }

    pub(crate) fn compare_and_swap_root(
        &mut self,
        account_locator: &str,
        request: AcsCompareAndSwapRootRequest,
        now_epoch_ms: i64,
    ) -> Result<AcsMemoryDelivery<AcsCompareAndSwapRootResponse>, AcsErrorResponse> {
        require_contract(&request.contract_id)?;
        request
            .replacement
            .validate()
            .map_err(|detail| error(AcsErrorCode::InvalidRequest, detail))?;
        let lose_response = self.lose_next_root_response;
        self.lose_next_root_response = false;
        let account = self.account_mut(account_locator)?;
        require_children(account, &request.replacement)?;
        let current_revision = account.root.as_ref().map_or(0, |root| root.revision);
        let current_root_hash = account.root.as_ref().map(|root| root.root_hash.clone());
        if request.expected_revision != current_revision
            || request.expected_root_hash != current_root_hash
        {
            return Ok(AcsMemoryDelivery::Delivered(
                AcsCompareAndSwapRootResponse::Conflict {
                    current_revision,
                    current_root_hash,
                },
            ));
        }
        let root = AcsRootSnapshot {
            revision: current_revision.saturating_add(1),
            root_hash: request
                .replacement
                .authenticated_hash(AcsEncryptedValueKind::Root, ACS_FIXED_ROOT_ID)
                .map_err(|detail| error(AcsErrorCode::InvalidRequest, detail))?,
            value: request.replacement,
            updated_at_epoch_ms: now_epoch_ms,
        };
        account.next_sequence = account.next_sequence.saturating_add(1);
        account.events.push(AcsSseEvent::RootChanged {
            sequence: account.next_sequence,
            root_revision: root.revision,
            root_hash: root.root_hash.clone(),
        });
        account.root = Some(root.clone());
        if lose_response {
            Ok(AcsMemoryDelivery::LostAfterCommit)
        } else {
            Ok(AcsMemoryDelivery::Delivered(
                AcsCompareAndSwapRootResponse::Committed { root },
            ))
        }
    }

    fn events_after(
        &self,
        account_locator: &str,
        last_sequence: u64,
    ) -> Result<Vec<AcsSseEvent>, AcsErrorResponse> {
        Ok(self
            .account(account_locator)?
            .events
            .iter()
            .filter(|event| event.sequence() > last_sequence)
            .cloned()
            .collect())
    }

    fn inject_lost_next_root_response(&mut self) {
        self.lose_next_root_response = true;
    }

    fn account(&self, locator: &str) -> Result<&AcsMemoryAccount, AcsErrorResponse> {
        self.accounts
            .get(locator)
            .ok_or_else(|| error(AcsErrorCode::NotFound, "account not found"))
    }

    fn account_mut(&mut self, locator: &str) -> Result<&mut AcsMemoryAccount, AcsErrorResponse> {
        self.accounts
            .get_mut(locator)
            .ok_or_else(|| error(AcsErrorCode::NotFound, "account not found"))
    }
}

fn require_contract(contract_id: &str) -> Result<(), AcsErrorResponse> {
    if contract_id == ACS_CONTRACT_ID {
        Ok(())
    } else {
        Err(error(
            AcsErrorCode::InvalidRequest,
            "unsupported ACS contract",
        ))
    }
}

fn require_children(
    account: &AcsMemoryAccount,
    value: &AcsEncryptedValue,
) -> Result<(), AcsErrorResponse> {
    if let Some(missing) = value
        .child_object_ids
        .iter()
        .find(|child| !account.objects.contains_key(*child))
    {
        Err(error(
            AcsErrorCode::MissingChildObject,
            format!("missing child object {missing}"),
        ))
    } else {
        Ok(())
    }
}

fn error(code: AcsErrorCode, message: impl Into<String>) -> AcsErrorResponse {
    AcsErrorResponse {
        contract_id: ACS_CONTRACT_ID.to_string(),
        request_id: "memory-provider".to_string(),
        code,
        message: message.into(),
        retry_after_ms: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCOUNT: &str = "account-a";

    fn object(id: &str, bytes: &[u8], children: &[&str]) -> AcsCreateObjectRequest {
        AcsCreateObjectRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            object_id: id.to_string(),
            value: AcsEncryptedValue::from_ciphertext(
                bytes,
                children.iter().map(|child| (*child).to_string()).collect(),
            ),
        }
    }

    fn root(
        expected_revision: u64,
        expected_root_hash: Option<String>,
        bytes: &[u8],
        children: &[&str],
    ) -> AcsCompareAndSwapRootRequest {
        AcsCompareAndSwapRootRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            expected_revision,
            expected_root_hash,
            replacement: AcsEncryptedValue::from_ciphertext(
                bytes,
                children.iter().map(|child| (*child).to_string()).collect(),
            ),
        }
    }

    fn provider() -> InMemoryAcsProvider {
        let mut provider = InMemoryAcsProvider::default();
        provider.create_account(ACCOUNT).unwrap();
        provider
    }

    #[test]
    fn create_once_is_idempotent_but_never_overwrites() {
        let mut provider = provider();
        assert_eq!(
            provider.create_object(ACCOUNT, object("page", b"one", &[]), 10),
            Ok(AcsCreateObjectOutcome::Created)
        );
        assert_eq!(
            provider.create_object(ACCOUNT, object("page", b"one", &[]), 11),
            Ok(AcsCreateObjectOutcome::AlreadyExists)
        );
        let error = provider
            .create_object(ACCOUNT, object("page", b"two", &[]), 12)
            .unwrap_err();
        assert_eq!(error.code, AcsErrorCode::ObjectIdCollision);
        assert_eq!(
            provider
                .read_object(ACCOUNT, "page")
                .unwrap()
                .unwrap()
                .value
                .ciphertext()
                .unwrap(),
            b"one"
        );
    }

    #[test]
    fn root_cas_has_one_winner_and_one_notification() {
        let mut provider = provider();
        provider
            .create_object(ACCOUNT, object("page-a", b"a", &[]), 10)
            .unwrap();
        provider
            .create_object(ACCOUNT, object("page-b", b"b", &[]), 10)
            .unwrap();

        let first = provider
            .compare_and_swap_root(ACCOUNT, root(0, None, b"root-a", &["page-a"]), 20)
            .unwrap();
        assert!(matches!(
            first,
            AcsMemoryDelivery::Delivered(AcsCompareAndSwapRootResponse::Committed { .. })
        ));
        let second = provider
            .compare_and_swap_root(ACCOUNT, root(0, None, b"root-b", &["page-b"]), 21)
            .unwrap();
        assert!(matches!(
            second,
            AcsMemoryDelivery::Delivered(AcsCompareAndSwapRootResponse::Conflict {
                current_revision: 1,
                ..
            })
        ));
        assert_eq!(provider.events_after(ACCOUNT, 0).unwrap().len(), 1);
        assert_eq!(provider.root(ACCOUNT).unwrap().unwrap().revision, 1);
    }

    #[test]
    fn staged_objects_are_silent_and_missing_references_are_rejected() {
        let mut provider = provider();
        provider
            .create_object(ACCOUNT, object("leaf", b"leaf", &[]), 10)
            .unwrap();
        assert!(provider.events_after(ACCOUNT, 0).unwrap().is_empty());
        let error = provider
            .create_object(ACCOUNT, object("branch", b"branch", &["missing"]), 11)
            .unwrap_err();
        assert_eq!(error.code, AcsErrorCode::MissingChildObject);
        let error = provider
            .compare_and_swap_root(ACCOUNT, root(0, None, b"root", &["missing"]), 12)
            .unwrap_err();
        assert_eq!(error.code, AcsErrorCode::MissingChildObject);
    }

    #[test]
    fn ambiguous_commit_is_resolved_by_reading_the_fixed_root() {
        let mut provider = provider();
        provider
            .create_object(ACCOUNT, object("page", b"page", &[]), 10)
            .unwrap();
        provider.inject_lost_next_root_response();
        let delivery = provider
            .compare_and_swap_root(ACCOUNT, root(0, None, b"root", &["page"]), 20)
            .unwrap();
        assert_eq!(delivery, AcsMemoryDelivery::LostAfterCommit);

        let observed = provider.root(ACCOUNT).unwrap().unwrap();
        assert_eq!(observed.revision, 1);
        assert_eq!(observed.value.child_object_ids, vec!["page"]);
        assert_eq!(provider.events_after(ACCOUNT, 0).unwrap().len(), 1);
    }

    #[test]
    fn event_cursor_replays_only_later_root_commits() {
        let mut provider = provider();
        provider
            .create_object(ACCOUNT, object("one", b"one", &[]), 10)
            .unwrap();
        let first = provider
            .compare_and_swap_root(ACCOUNT, root(0, None, b"root-1", &["one"]), 20)
            .unwrap();
        let AcsMemoryDelivery::Delivered(AcsCompareAndSwapRootResponse::Committed {
            root: first_root,
        }) = first
        else {
            panic!("first root commit did not complete")
        };
        provider
            .create_object(ACCOUNT, object("two", b"two", &[]), 30)
            .unwrap();
        provider
            .compare_and_swap_root(
                ACCOUNT,
                root(
                    first_root.revision,
                    Some(first_root.root_hash),
                    b"root-2",
                    &["two"],
                ),
                40,
            )
            .unwrap();

        let replay = provider.events_after(ACCOUNT, 1).unwrap();
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].sequence(), 2);
    }
}
