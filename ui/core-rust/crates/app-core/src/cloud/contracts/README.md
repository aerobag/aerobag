# Frozen Cloud Contracts

These files describe the persisted account, not platform UI messages. They are
append-only once committed. `check_cloud_contract_history.py` checks their full
Git history; hosted jobs running it must use `fetch-depth: 0`.

The record registry's actual wire types generate the inventory recursively.
Changes to nested fields and enum variants count, not only top-level version
constants. `account_contracts` tests compare it with the current numbered file
and require an explicit migration for each changed record family.

For a new contract, retain the old descriptor/decoders, declare the successor
and migration policies, and add its newly numbered JSON file. To print a
candidate, run the ignored `cloud::tests::account_contracts::print_new_account_contract`
test with `--ignored --nocapture`. There is deliberately no snapshot-update mode.

Format 1 predates the inventory and was already published with multiple
flight-plan schemas. Its file records known historical versions, not a claim
that a recursive frozen shape existed then. The format-2 migration explicitly
discards old crossfill without parsing its flight plan. All subsequent files
contain full recursive schemas derived from the frozen wire types.
