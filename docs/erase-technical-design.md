# Erase: Lineage-Driven Erasure Orchestrator — Prototype Technical Design

2026-11-06

## 1. Summary

Erase is an open-source orchestrator that takes a data subject identifier, walks column-level lineage to find every copy of that person's data, executes a reviewed deletion plan, and produces tamper-evident proof of what was done and what remains. The prototype proves the full loop on one real request across Postgres and Snowflake in 6 to 8 weeks.

**Problem.** Catalogs such as DataHub, OpenLineage/Marquez and OpenMetadata already know where data flows, but nothing open-source turns that graph into an executed, verified erasure. Teams fall back to ticket-driven manual deletion across systems, which is slow, unevidenced and misses derived copies.

**Core loop.**

```
Subject ID -> Lineage walk -> Classify -> Plan -> Soft-delete -> Approve -> Hard-delete -> Verify -> Audit
```

**What the prototype demonstrates.**

- One subject ID resolved to columns and rows in a source Postgres database and downstream Snowflake tables, found through DataHub lineage.
- A dry-run plan with a per-step action (delete, mask, recompute, retain) and a legal basis for every retained item.
- A two-phase saga with a human approval gate, then verification including a quasi-identifier re-identification attempt.
- A hash-chained audit log exported as a signed PDF and CSV.

**Assumptions.** Identity verification of the requester, request intake, and the decision on whether erasure applies happen upstream. Erase executes an approved decision. This is an engineering design, not legal advice; the legal mappings below need review by a DPO or counsel.

## 2. Regulatory basis, with corrections to the brief

Several statements in the source brief need adjusting before they go into product claims or a README. The design assumes the corrected versions.

| Brief says | Corrected position | Design impact |
| --- | --- | --- |
| GDPR "30-day deadline" | GDPR Art. 12(3): respond without undue delay and within one month, extendable by two further months for complex or numerous requests. CCPA/CPRA uses 45 days, extendable by 45. | The deadline is a per-request configurable SLA clock, not a constant. Alerts at 50% and 80% of the window. |
| Financial retention "under Art. 21(3)" | Art. 21 is the right to object. Erasure and its exceptions are Art. 17; exceptions are in Art. 17(3), including legal obligation (b), archiving and research (d) and legal claims (e). | Retention rules use a legal-basis enum keyed to Art. 17(3) sub-points plus a free-text citation of the specific national or sectoral law. |
| HIPAA and DMA as erasure drivers | HIPAA gives no general right to erasure; it mainly creates retention duties (for example six-year documentation retention). The DMA is competition law and does not drive erasure. | HIPAA is a source of retention exceptions, not a trigger. DMA is out of scope. |
| EU AI Act "Aug 2027" | The Digital Omnibus on AI reached provisional agreement in May 2026 and is reported as in force. It moves standalone high-risk (Annex III) obligations to 2 December 2027 and product-embedded (Annex I) to 2 August 2028. Verify against the Official Journal before publishing. | Treat AI Act support as roadmap. The Act does not itself mandate lineage-driven erasure; it raises documentation and data-governance expectations. |
| Manual DSAR cost of ~$1,400 | Unverified figure; no primary source found. | Do not use in public materials until sourced. Measure cost per request in the pilot instead. |
| Competitor feature claims (BigID, DataGrail, `forgettrace`, `SlateGitOrg/erasure`) | Not independently checked in this document. | Validate each cell of the comparison table before publishing it. |

**Provisions the engine must model**

- **Art. 17(1)** right to erasure and the grounds under which it applies.
- **Art. 17(2) and Art. 19** duty to inform other controllers and recipients. The plan includes a NOTIFY step for downstream processors and recipients.
- **Art. 17(3)** exceptions, each stored with basis, citation and review or expiry date.
- **Art. 5(2) and Art. 30** accountability and records, which the audit trail serves.
- **Art. 32** security of processing, which constrains how Erase itself handles identifiers.

## 3. Goals and non-goals

**Goals for the prototype**

1. Discover downstream copies of a subject's data from column-level lineage (DataHub first).
2. Generate a deterministic, reviewable erasure plan and run it in dry-run mode by default.
3. Execute against Postgres and Snowflake through a documented adapter interface.
4. Verify post-erasure state, including residual copies (time travel, backups) and a re-identification attempt.
5. Produce an audit trail that an auditor can check independently.

**Non-goals for the MVP**

- Request intake, identity proofing and any portal UI. Integrate with existing DSAR tools instead.
- Machine unlearning. Models and feature stores appear in the plan as flagged manual steps with lineage evidence.
- Discovery of data the catalog does not know about (shadow IT, unmanaged exports). The engine reports coverage gaps instead.
- Legal determination of whether erasure applies. Policy rules are inputs authored by the DPO.

## 4. Architecture

Erase is a control plane that reads from catalogs and writes through adapters; it never stores subject data beyond what a request needs. All state transitions go through one persisted workflow engine so every step is resumable and auditable.

```
            +-------------------------------+
  DSAR  --> |  API / CLI  (FastAPI, Typer)  |
  tool      +---------------+---------------+
                            |
                 +----------v-----------+      +-------------------+
                 |  Workflow engine     |<---->|  Control-plane DB |
                 |  (saga state machine)|      |  (Postgres)       |
                 +--+----+----+----+----+      +-------------------+
                    |    |    |    |
     +--------------+    |    |    +----------------+
     v                   v    v                     v
+-----------+   +-------------+ +----------+   +------------+
| Discovery |   | Classifier  | | Planner  |   | Audit log  |
| LineageSrc|   | + policy    | | (pure fn)|   | hash chain |
+-----+-----+   +-------------+ +----------+   +-----+------+
      |                                              |
 DataHub / OpenLineage                      PDF / CSV export, anchor

  Execution: Adapter SDK  ->  Postgres | Snowflake | (S3, BigQuery ... later)
  Verification: re-discovery + residual check + re-identification probe
```

**Design principles**

- **Planner is a pure function.** Same lineage snapshot, policy and subject yields the same plan and the same plan digest. This makes dry-run output reviewable and approvals binding.
- **Plan before power.** Discovery and planning use read-only credentials. Write credentials are only loaded by the executor after approval.
- **Every external effect is idempotent and recorded** before and after it runs.
- **Catalog is a source, not the truth.** The engine reports confidence and coverage gaps rather than assuming lineage is complete.

**Technology choices for the prototype**

| Concern | Choice | Reason |
| --- | --- | --- |
| Language | Python 3.12 | DataHub and OpenLineage SDKs, Snowflake and Postgres drivers, data-team familiarity |
| Workflow | Own state machine persisted in Postgres (transactional outbox) | Avoids a heavy dependency in the MVP. Temporal is the upgrade path once multi-day sagas and retries matter. |
| API | FastAPI, plus a CLI for scripting | Same service layer for both |
| Plan and policy format | JSON Schema-validated YAML/JSON | Diffable and signable |
| Audit signing | Ed25519 (PyNaCl) and PDF signing with pyHanko | Small, auditable libraries |
| Packaging | Docker image, Helm chart later | Self-hosted by default |

## 5. Core data model and interfaces

The control-plane database holds workflow state and audit entries, never raw subject identifiers in clear text.

| Table | Key fields | Purpose |
| --- | --- | --- |
| `erasure_request` | id, subject_ref (HMAC), identifier_types, received_at, deadline_at, status | One row per approved erasure decision |
| `lineage_snapshot` | id, request_id, source, captured_at, graph (JSON), digest | Frozen graph the plan was computed from |
| `plan` | id, request_id, snapshot_id, policy_version, digest, status | Immutable once created; approvals bind to `digest` |
| `plan_step` | id, plan_id, system, target, action, predicate, legal_basis, depends_on | One executable or retained item |
| `step_attempt` | id, step_id, phase (soft/hard/verify), outcome, rows_affected, started/ended | Retryable execution history |
| `retention_rule` | id, scope, legal_basis, citation, expires_at, owner | DPO-authored exceptions |
| `audit_entry` | seq, prev_hash, hash, payload, signature | Hash chain (section 11) |

**Adapter SDK (Python protocol)**

```python
class Adapter(Protocol):
    def capabilities(self) -> Capabilities: ...           # delete, mask, restore, residuals
    def locate(self, step: PlanStep) -> LocateResult: ...  # row count + keyed digest, no PII out
    def soft_delete(self, step: PlanStep, ctx: ExecCtx) -> StepResult: ...
    def restore(self, step: PlanStep, ctx: ExecCtx) -> StepResult: ...      # compensation
    def hard_delete(self, step: PlanStep, ctx: ExecCtx) -> StepResult: ...
    def verify(self, step: PlanStep, ctx: ExecCtx) -> VerifyResult: ...
    def residuals(self, step: PlanStep) -> list[Residual]: ...  # time travel, replicas, backups
```

```python
class LineageSource(Protocol):
    def seed(self, identifier_type: str) -> list[ColumnRef]: ...
    def downstream(self, col: ColumnRef, depth: int) -> list[Edge]: ...  # Edge carries transform type
```

Every adapter result includes counts and a keyed digest of affected keys, so verification and audit can reference records without exposing them.

## 6. Discovery layer

Discovery turns a subject identifier type into a set of columns, then into rows per system. Column-level lineage tells Erase which columns derive from the identifier; it does not tell it which rows belong to the subject, so the walk has two stages.

**Stage 1: column walk**

1. **Seed.** The operator maps each identifier type (email, customer_id, phone) to seed columns in the system of record. Seeds can also come from a PII tag in the catalog.
2. **Traverse downstream** through fine-grained lineage. DataHub exposes this as fine-grained lineage on dataset edges; OpenLineage carries it in the column lineage facet, including transformation type.
3. **Classify each edge by transformation type**, which decides what the planner can do:

| Edge type | Meaning | Row-level erasure possible? |
| --- | --- | --- |
| IDENTITY / passthrough | Key survives unchanged | Yes, same predicate |
| TRANSFORMATION (hash, tokenize) | Key is encoded | Yes, if Erase can compute the encoding (known HMAC key or lookup table); otherwise flagged |
| JOIN / FILTER (indirect) | Column influences which rows exist | Review: rows may reference the subject indirectly |
| AGGREGATION | Many subjects collapsed | Not row-level. Recompute after source deletion, or retain if proven anonymous |

4. **Stop conditions.** Depth limit, cycle detection, and a node cap, all reported so truncated walks are visible.

**Stage 2: row resolution.** For each reachable column, the adapter builds a predicate (`customer_id = :id` or the encoded form) and calls `locate()` to get a count and keyed digest. Zero hits is still recorded as evidence.

**Catalog integration**

- **DataHub** first, through its GraphQL API (or the DataHub MCP server as a transport; both sit behind `LineageSource`).
- **OpenLineage/Marquez and OpenMetadata** second, behind the same interface. Core never imports a catalog SDK.

**Coverage report.** Each request records: systems reached, systems with PII tags that lineage did not reach, and datasets with no column-level lineage. This turns "is lineage complete?" into a visible number the DPO signs off on.

## 7. Classification and retention exceptions

Classification decides, per column, whether it identifies the subject directly, indirectly (quasi-identifier), or not at all, and whether a legal exception stops deletion.

**Column classes**

- **Direct identifier:** email, name, government ID, account number. Always in scope.
- **Quasi-identifier:** date of birth, postcode, gender, device ID, rare job title. In scope for masking and for the re-identification test.
- **Sensitive (GDPR Art. 9):** health, biometrics, beliefs. Flagged for stricter approval.
- **Non-personal:** ignored but listed in the plan as reviewed.

Sources: catalog PII tags first, then built-in regex and checksum detectors (email, IBAN, phone), then an optional ML classifier plugin. Every classification carries a confidence value and its source; low-confidence columns go to a human queue instead of silently passing.

**Retention exceptions are structured, not boolean.** A rule is only valid if it carries all of these fields:

| Field | Example |
| --- | --- |
| scope | `billing.invoices` where `customer_id = subject` |
| legal_basis | `GDPR_17_3_b_legal_obligation` |
| citation | National tax code section the DPO cites |
| retain_until | Date the obligation ends; the engine re-queues deletion then |
| minimization | Columns that can still be masked inside the retained row |
| owner | Named approver for the rule |

A retained item is never silently skipped. It appears in the plan as `RETAIN` with its rule, is restricted from further processing where the adapter supports it (for example a row-level flag or a view that hides it), and is scheduled for deletion on `retain_until`. Rules are versioned; the plan records the policy version used.

Policy is authored by the DPO as reviewed YAML in version control. Erase evaluates it; it does not decide what the law requires.

## 8. Planning

The planner converts the lineage snapshot, classification and policy into an ordered list of steps. It is a pure function, so a reviewer can diff two plans and an approval can bind to a digest.

**Step actions**

| Action | When used | Typical mechanism |
| --- | --- | --- |
| DELETE | Row-level key available, no exception | `DELETE ... WHERE`, object delete, tombstone |
| MASK | Row must stay (exception or referential integrity), identifiers need removing | Null or pseudonymize direct identifiers and quasi-identifiers |
| RECOMPUTE | Derived or aggregate data built from deleted rows | Trigger the pipeline rebuild after the source step succeeds |
| RETAIN | Valid exception rule applies | No change, restrict, schedule future deletion |
| NOTIFY | Recipient or processor holds a copy (Art. 17(2), 19) | Generate notice for the DPO to send |
| MANUAL | No adapter, or model or feature-store dependency | Task with lineage evidence and owner |

**Ordering.** Steps form a DAG. Sources are erased before derived tables are recomputed, and RECOMPUTE steps depend on the upstream DELETE completing. Independent systems run in parallel.

**Guardrails computed at plan time**

- Blast-radius check: abort the plan if any predicate would touch more than a configured share of a table (default 0.1%) or more than N rows per subject, which catches wrong joins and null keys.
- Reject predicates that reference no subject key.
- Warn when a table has no primary key or when a delete would violate a foreign key and name the cascade.

**Example plan fragment**

```yaml
plan:
  request: er_2026_0412
  snapshot: ls_88af
  policy_version: 14
  digest: sha256:9c41...e0
  steps:
    - id: s1
      system: postgres://prod/app
      target: public.users
      action: DELETE
      predicate: "id = :subject"
      expected_rows: 1
    - id: s2
      system: postgres://prod/app
      target: public.invoices
      action: MASK
      columns: [billing_name, billing_email]
      legal_basis: GDPR_17_3_b_legal_obligation
      citation: "Tax records retention, 7y"
      retain_until: 2033-03-31
    - id: s3
      system: snowflake://wh/analytics
      target: marts.customer_360
      action: RECOMPUTE
      depends_on: [s1, s2]
    - id: s4
      system: mlflow://models/churn_v3
      action: MANUAL
      reason: "trained on marts.customer_360; unlearning not supported"
```

Dry-run output is this plan plus counts from `locate()`. Nothing is written until a plan is approved.

## 9. Execution: two-phase saga with an approval gate

Deletion is irreversible, so the saga makes the first phase reversible and puts a human decision before the second.

```
PLANNED --approve(plan digest)--> SOFT_RUNNING --> SOFT_DONE
                                       |               |
                                  fail |        hold window (<= 7 days default)
                                       v               |
                                COMPENSATING <--reject--+--approve(hard)--> HARD_RUNNING
                                       |                                        |
                                    RESTORED                                 HARD_DONE
                                                                                |
                                                                        VERIFYING --> CLOSED
                                                                            |
                                                                         FAILED_VERIFY --> retry / escalate
```

**Phase 1: soft delete (reversible)**

- Rows are moved to a quarantine table or schema in the same system, encrypted with a per-request data key, and removed from the live table. Object stores use a delete marker with versioning kept.
- If any step fails, executed steps are compensated with `restore()` and the request returns to PLANNED with the error.
- The quarantine is still personal data. It lives only for the hold window and is covered by the request's own deadline; the hold window default must leave time inside the statutory period.

**Approval gate**

- Approval is on the plan digest. If lineage or policy changes and the plan is regenerated, the old approval is void.
- Four-eyes by default: the approver cannot be the person who started the run. Approver, time and comment go to the audit log.
- Dry-run is the default mode of every command. Executing needs an explicit flag and an approved plan.

**Phase 2: hard delete (irreversible)**

| System | Hard-delete mechanism | Known residual to record |
| --- | --- | --- |
| Postgres | `DELETE` of quarantine rows, drop the per-request key, `VACUUM` the table | WAL archives, replicas, PITR backups until they expire |
| Snowflake | `DELETE`, drop quarantine, shorten or bypass Time Travel where policy allows | Time Travel (up to the table's retention days) then 7 days of Fail-safe on permanent tables |
| Object store (later) | Delete all versions, remove delete markers | Replication targets, lifecycle delay |
| Kafka (later) | Compacted-topic tombstone, or crypto-shred by key | Uncompacted segments, consumers' local state |

Crypto-shredding is used where possible: if the quarantine or a record set is encrypted under a per-request key, destroying the key makes remaining copies unreadable, which covers backups that cannot be edited.

**Backups.** Backups cannot usually be edited. The design records each backup's expiry date as a residual and keeps a minimal suppression entry (keyed hash of the identifier) so that a restore re-triggers deletion. Whether a suppression list is lawful for your jurisdiction needs DPO sign-off (see section 15).

**Reliability rules.** Each step is idempotent, retried with backoff, and writes its intent and outcome to the control-plane DB before and after the external call. A crashed worker resumes from the last recorded state.

## 10. Verification

Verification produces evidence that the plan achieved its goal inside the known data graph. It cannot prove absence everywhere, and reports say so.

**Three checks**

1. **Re-discovery.** Re-run the original lineage walk and every `locate()` predicate. Expected result: zero rows for DELETE and MASK steps, unchanged counts for RETAIN steps. Compare against the pre-erasure snapshot.
2. **Residual inventory.** Ask each adapter for `residuals()`: Time Travel windows, replicas, backup expiry, search indexes, caches. Each is recorded with an end date. Verification passes only when residuals are either absent or have a dated, documented expiry within policy.
3. **Re-identification probe.** Attempt to reconstruct the subject from what remains.

**Re-identification probe design**

- Before deletion, build a *fingerprint* of the subject: the values of their quasi-identifiers (for example birth date, postcode, gender, device ID, rare attributes), held encrypted under the per-request key and used only for the probe.
- After deletion, the probe queries every in-scope dataset for rows matching the fingerprint on at least k of the quasi-identifiers, and joins across datasets on shared keys.
- Output per dataset: candidate row count and whether the fingerprint is *unique* (exactly one candidate). A unique match is a failure and blocks closure. A match among many records is reported but passes, since the person is not singled out.
- The fingerprint is destroyed when the request closes, by dropping its key.

**Limits stated in every report**

- Coverage depends on lineage completeness. The report lists systems and datasets that were not reached.
- The probe tests a defined attack (matching on known quasi-identifiers), not every possible attack.
- Aggregates and trained models are out of the probe's scope in the MVP.

**Result states:** VERIFIED, VERIFIED_WITH_RESIDUALS (dated expiries listed), or FAILED with the failing checks. Only the first two allow the request to close, and the second schedules a follow-up verification for each residual's expiry date.

## 11. Audit trail

The audit log is append-only and hash-chained, so any later edit or deletion of an entry is detectable. It is evidence for the accountability principle (GDPR Art. 5(2)), not a compliance guarantee on its own.

**Entry structure**

```json
{
  "seq": 1042,
  "ts": "2026-11-06T18:22:41Z",
  "request": "er_2026_0412",
  "subject_ref": "hmac-sha256:7b3e...",
  "event": "HARD_DELETE_STEP_DONE",
  "system": "snowflake://wh/analytics",
  "target": "marts.customer_360",
  "rows_affected": 3,
  "legal_basis": null,
  "operator": "alice@example.com",
  "approver": "bob@example.com",
  "plan_digest": "sha256:9c41...e0",
  "prev_hash": "sha256:aa10...",
  "hash": "sha256:51d9..."
}
```

**Chain rule.** `hash = SHA-256(prev_hash || canonical_json(entry without hash))`, using RFC 8785 canonical JSON so hashing is reproducible.

**Design decisions**

- **Subject reference is a keyed HMAC, not a plain hash.** A plain SHA-256 of an email can be reversed by guessing. The HMAC key is a tenant secret held outside the log, so the log alone cannot be used to identify the person.
- **Signed checkpoints.** Every N entries, and at request close, the chain head is signed with an Ed25519 key.
- **External anchoring.** A chain in a database the operator controls can be rewritten end to end. To make tampering detectable, publish the signed head to an independent location: an object store with Object Lock, an RFC 3161 timestamp authority, or a transparency log.
- **Minimal content.** No raw identifiers, no row contents. Only counts, systems, bases and people.

**Exports**

- **CSV and JSON:** the entire chain for a request, plus a `verify-chain` CLI command that recomputes hashes and checks signatures offline.
- **PDF report:** executive summary, plan, per-system outcomes, retained items with legal basis, residuals with dates, verification results, coverage gaps, chain head hash and signature. Rendered to PDF/A and signed (pyHanko) so alteration is visible in a PDF reader.

The log is tamper-evident, not tamper-proof: its strength comes from key custody and external anchoring, and the report states which of those were configured.

## 12. MVP adapters

| Capability | Postgres adapter | Snowflake adapter |
| --- | --- | --- |
| Connection | psycopg 3, role `erase_exec` with column-scoped grants | Snowflake connector, key-pair auth, role `ERASE_EXEC` |
| locate | `SELECT count(*), keyed digest of PKs` with predicate | Same, run on a dedicated small warehouse |
| Soft delete | `INSERT INTO erase_quarantine.<t> SELECT ... ; DELETE ...` in one transaction, row payload encrypted with the request key | `CREATE TABLE erase_quarantine.<t> AS SELECT ...`; `DELETE`; quarantine encrypted at the application layer or by a dedicated key |
| Restore | Reverse insert in one transaction | Reverse insert |
| Hard delete | Drop quarantine rows and key; `VACUUM` the table | Drop quarantine; optionally reduce `DATA_RETENTION_TIME_IN_DAYS` on affected tables where policy allows |
| Mask | `UPDATE` columns to NULL or a deterministic pseudonym | Same, plus masking policy where one exists |
| Residuals | WAL archive, replica lag, backup list and expiry | Time Travel window per table, Fail-safe 7 days for permanent tables, shares and clones |
| Recompute trigger | Optional `REFRESH MATERIALIZED VIEW` | Trigger task or dbt job via webhook |

**Known hazards the adapters handle**

- Foreign keys: the planner orders child tables first or uses documented cascades.
- Clones and shares in Snowflake copy data. Discovery lists them as residual targets because lineage often omits them.
- Triggers or CDC on Postgres can re-propagate a delete or a restore. The adapter reports active publications and logical slots as a warning.

**Community adapters.** The SDK ships a conformance test suite: given a fixture dataset, the adapter must pass locate, soft delete, restore, hard delete and verify. An adapter that fails any case cannot be marked `stable` in the registry.

## 13. Security and threat model

Erase holds credentials that can delete production data and sees identifiers of people, so it is a high-value target. These controls are in scope for the prototype.

| Threat | Control |
| --- | --- |
| Predicate bug deletes the wrong rows | Dry-run default, blast-radius limits, `expected_rows` check, quarantine with restore before hard delete |
| Stolen executor credentials | Separate read-only discovery role and write role per system; write credentials fetched from a secret manager only at execution; short-lived tokens where the platform allows |
| Insider approves their own request | Four-eyes enforcement, approval bound to plan digest, separation between requester, approver and operator roles |
| Plan altered after approval | Executor re-computes and compares the digest before each step |
| Audit log rewritten | Hash chain, signed checkpoints, external anchoring (section 11) |
| Erase leaks the subject's data | Identifiers encrypted at rest, HMAC in logs, no row contents in audit or traces, log scrubbers in CI tests |
| Lineage poisoned or stale | Snapshot is recorded; staleness threshold fails the plan if the snapshot is older than a set age |
| Supply chain via community adapters | Signed adapter releases, capability declaration, adapters run with only their own system's credentials, conformance suite |

**Authentication and authorization.** OIDC login, roles `requester`, `approver`, `operator`, `auditor`, enforced on the API and CLI.

**Data minimization in Erase itself.** The control plane stores subject references as keyed hashes. Raw identifiers are accepted only to run the walk, held encrypted for the length of the request, and deleted at close. Erase is itself in scope of a records-of-processing entry (Art. 30).

## 14. MVP plan and milestones

Eight weeks with one to two engineers. Six weeks is achievable only if audit export and hardening (weeks 7 and 8) are cut to a minimum.

| Weeks | Deliverable | Exit criterion |
| --- | --- | --- |
| 1 | Repo, CI, control-plane schema, workflow state machine, synthetic demo dataset (Postgres source, dbt models in Snowflake, seeded PII) | State machine passes unit tests; demo dataset loads in one command |
| 2 | DataHub `LineageSource`, column walk with transformation types, coverage report | Walk finds all seeded downstream columns; gaps listed |
| 3 | Classifier, policy YAML, retention rules, planner with digest and guardrails | Same inputs give identical digest; dry-run plan printed |
| 4 | Postgres adapter, soft delete, restore, approval gate | Full soft-delete and restore on the demo, including failure injection |
| 5 | Snowflake adapter, hard delete, residual reporting | End-to-end run on both systems |
| 6 | Verification: re-discovery, residuals, re-identification probe | Probe catches a deliberately left-behind unique row |
| 7 | Audit chain, checkpoints, `verify-chain`, signed PDF and CSV export | Offline tool detects a single edited entry |
| 8 | Hardening, docs, adapter SDK conformance suite, demo script | Fresh clone runs the demo; the controls for wrong-row deletion, plan tampering and log tampering in section 13 each have a passing test |

**Definition of done for the prototype**

- One end-to-end erasure on the demo, with an approved plan, a passed verification and a signed report.
- Precision and recall of discovery measured against the seeded ground truth (target: no false deletes in the demo, recall at or above 95% on seeded columns).
- A written list of everything the prototype does not cover (section 3 and section 15).

**After the MVP (roadmap)**

1. OpenLineage/Marquez and OpenMetadata sources; BigQuery and S3 adapters.
2. Kafka crypto-shredding and Elasticsearch adapters.
3. Model and feature-store lineage: flag affected models, track retrain tickets, and produce documentation for AI Act data-governance records. Actual unlearning stays out until the techniques are production-grade.
4. Multi-tenant hosted option, if a managed offering is pursued.

## 15. Risks and open questions

| Risk | Likelihood | Mitigation |
| --- | --- | --- |
| Column-level lineage is missing or wrong in real catalogs | High | Coverage report; sampling scans for PII outside the graph; treat lineage as a hint that needs sign-off |
| Snowflake and warehouse retention make "hard delete" take days | Certain | Residual tracking with dated expiries; follow-up verification |
| Hashed or tokenized keys cannot be matched downstream | Medium | Key-resolver plugin; MANUAL fallback with evidence |
| Re-identification probe gives false confidence | Medium | State the attack model in every report; add attack variants over time |
| Scope creep into DSAR intake or model unlearning | High | Non-goals in section 3 enforced at review |
| Name conflict: "Erase" is generic and likely taken | High | Pick a distinctive name and check trademarks before the first public release |

**Open questions for the owner**

1. **Suppression list.** Is keeping a keyed hash of erased identifiers to re-apply deletion after backup restore acceptable to your DPO or counsel?
2. **Hold window.** What soft-delete window fits your deadline policy (default 7 days)?
3. **Who authors retention rules**, and how are they reviewed and versioned?
4. **Processor notifications.** Should Erase send notices to processors (Art. 19) or only generate them?
5. **Pilot environment.** Which real Postgres and Snowflake instances and which DataHub deployment will the first demo run against?
6. **Licensing.** Apache 2.0 is proposed for the core and SDK. Confirm contributor terms (DCO or CLA) before accepting outside adapters.
7. **Competitive table.** Which claims about BigID, DataGrail, `forgettrace` and `SlateGitOrg/erasure` can be verified before they are published?

## 16. Sources

As of 2026-11-06. The AI Act timeline comes from web search results (summaries of the pages, not full reads); confirm against the Official Journal. GDPR, HIPAA, CCPA and Snowflake behaviour described above come from the author's working knowledge and need legal and vendor-documentation confirmation.

- [Gibson Dunn: EU AI Act Omnibus agreement, postponed high-risk deadlines](https://www.gibsondunn.com/eu-ai-act-omnibus-agreement-postponed-high-risk-deadlines-and-other-key-changes/)
- [CMS: The EU's Digital Omnibus on AI, what the provisional agreement changes](https://cms.law/en/gbr/publication/The-EU-s-Digital-Omnibus-on-AI-What-the-Provisional-Agreement-Really-Changes)
- [Sidley: EU lawmakers reach provisional agreement to delay key AI Act obligations](https://datamatters.sidley.com/2026/06/22/eu-lawmakers-reach-provisional-agreement-to-delay-key-eu-ai-act-obligations/)
- [K&L Gates via Mondaq: EU Digital Omnibus on AI enters into force](https://www.mondaq.com/eu-digital-omnibus-on-ai-enters-into-force/1825722)

## Appendix: Terminology

Terms used in this design, in alphabetical order. Legal terms are summarized for engineering use and are not legal advice.

| Term | Definition | Section |
| --- | --- | --- |
| Adapter | Plugin that lets Erase locate, delete, mask, restore and verify data in one system type (for example Postgres or Snowflake). | 5, 12 |
| Aggregation edge | Lineage edge where many rows or subjects collapse into one value; cannot be erased row by row and usually needs RECOMPUTE. | 6 |
| Anonymization | Irreversibly removing the link to a person so the data is no longer personal data. Stronger than pseudonymization; hard to prove in practice. | 8 |
| Anchoring | Publishing a signed audit-chain head to an independent location (Object Lock store, timestamp authority) so rewriting the log is detectable. | 11 |
| Approval gate | Human decision, bound to a plan digest, required before the hard-delete phase. | 9 |
| Audit entry | One record in the hash-chained log: event, system, counts, legal basis, operator, hashes. | 11 |
| Blast radius | Share of a table or number of rows a predicate would affect; used as a guardrail against wrong deletions. | 8 |
| Column-level lineage | Catalog metadata showing which source columns each downstream column is derived from. | 6 |
| Compensation | Undoing a completed saga step (`restore()`) when a later step fails or the plan is rejected. | 9 |
| Controller / processor | Under GDPR, the controller decides why and how data is processed; a processor handles it on the controller's behalf. Both may hold copies to erase. | 2, 8 |
| Coverage report | Per-request list of systems reached and systems or datasets that lineage did not reach. | 6 |
| Crypto-shredding | Making data unreadable by destroying the encryption key instead of editing every copy; useful for backups. | 9 |
| Data subject | The identified or identifiable person whose data is being erased. | 1 |
| Direct identifier | Value that identifies a person on its own: email, name, government ID. | 7 |
| Dry-run | Mode that computes a plan and row counts without writing anything; the default for every command. | 8, 9 |
| DSAR | Data subject access request. Used loosely here for any data subject rights request, including erasure. | 1 |
| Erasure plan | Immutable, ordered set of steps (DELETE, MASK, RECOMPUTE, RETAIN, NOTIFY, MANUAL) generated for one request. | 8 |
| Fail-safe | Snowflake's non-configurable 7-day recovery period after Time Travel for permanent tables; data is not directly queryable but still exists. | 9, 12 |
| Fingerprint | Encrypted set of a subject's quasi-identifier values, held only to run the re-identification probe, then destroyed. | 10 |
| Four-eyes | Rule that the approver must be a different person from the one who started the run. | 9, 13 |
| Hard delete | Irreversible removal phase: drop quarantine and keys, vacuum or tombstone. | 9 |
| Hash chain | Log structure where each entry's hash includes the previous entry's hash, so any edit breaks every later hash. | 11 |
| HMAC | Keyed hash. Used for subject references so the log cannot be reversed by guessing identifiers. | 11 |
| Hold window | Time between soft delete and hard delete when the deletion can still be reversed. | 9 |
| Legal basis | Documented ground for retaining data despite an erasure request, for example GDPR Art. 17(3)(b). | 2, 7 |
| LineageSource | Interface that reads lineage from a catalog (DataHub, OpenLineage, OpenMetadata). | 5, 6 |
| Lineage snapshot | Frozen copy of the lineage graph that a plan was computed from. | 5 |
| Locate | Adapter call returning a row count and keyed digest for a predicate, without returning personal data. | 5, 6 |
| Plan digest | SHA-256 over the canonical plan; approvals and execution are bound to it. | 8, 9 |
| Pseudonymization | Replacing identifiers with a token so data cannot be attributed without extra information. Still personal data under GDPR. | 6, 8 |
| Quarantine | Encrypted holding area for soft-deleted rows during the hold window. | 9 |
| Quasi-identifier | Value that does not identify a person alone but can in combination: birth date, postcode, gender, device ID. | 7, 10 |
| Re-identification probe | Post-erasure test that tries to single out the subject from remaining data by matching quasi-identifiers. | 10 |
| Residual | Copy that persists after hard delete for a known period: backups, replicas, Time Travel, Fail-safe. | 9, 10 |
| Retention exception | Structured rule (scope, legal basis, citation, retain-until date, owner) that stops deletion of specific data. | 7 |
| Saga | Workflow of steps that each have a compensating action, used instead of a distributed transaction. | 9 |
| Seed column | Column in a system of record mapped to an identifier type; the starting point of the lineage walk. | 6 |
| Soft delete | Reversible first phase: rows moved to quarantine and removed from live tables. | 9 |
| Subject reference | HMAC of the subject identifier used in the control plane and audit log in place of the raw value. | 5, 11 |
| Suppression entry | Minimal keyed hash of an erased identifier kept so a backup restore or re-ingest triggers deletion again. Needs DPO approval. | 9, 15 |
| Time Travel | Snowflake feature that keeps historical table data for a configured number of days. | 9, 12 |
| Tombstone | Marker record (for example in Kafka) that tells compaction to remove earlier records for a key. | 9 |
| Transformation type | Lineage edge classification: identity, transformation, join or filter, aggregation. Determines what the planner can do. | 6 |
| Verification states | VERIFIED, VERIFIED_WITH_RESIDUALS, FAILED. | 10 |
