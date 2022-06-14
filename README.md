# Payment Engine

A simple payments engine written in Rust.

## Usage

```sh
[RUST_LOG=debug] cargo run -- <input file> [> <output file>]
```

## Tests

```sh
cargo test
```

## Implementation Details

**Correctness & Assumptions**

- Transaction handling is checked via unit tests and the type system is used to
  ensure the correctness of incoming records (via `csv::Reader::deserialize`).
- Overall correctness of the program is also checked with a few integration
  tests covering valid and invalid inputs as well as some edge cases (e.g.
  nonexistent client IDs on withdrawals). The input CSVs used can be found in
  `examples/`.
- One edge case not mentioned in the spec is how to handle disputes, resolves
  and chargebacks when the given client ID doesn't match the original
  transaction. Here I assume it's an error and disregard the incoming
  transaction.
- It is possible for a client to have negative funds. If a client deposits,
  withdraws, and then a dispute and chargeback occur on the original deposit
  (e.g. perhaps the original deposit was fraudulent), then the client will be in
  debt by the amount in the withdrawal. I believe this is an legitimate flow.
- Disputes are only possible on deposits. This is implied by the spec: "clients
  available funds should decrease by the amount disputed, their held funds
  should increase by the amount disputed". It would otherwise be appropriate to
  dispute an erroneous/malicious withdrawal, but given the spec, I have not
  implemented this.
- Rounding errors are not accounted for here but the risks should be assessed in
  code intended for production.

**Error Handling and Safety**

- The repo contains no `unsafe` code or `unwrap`s.
- For the sake of brevity all runtime errors are handled via `anyhow` and
  bubbled up to the program entrypoint, where the program exits showing the
  error message. In a production setting such errors should be logged with
  details and appropriate actions taken (should we ignore the transaction?
  Rollback? Inform a partner for further action?).
- There are no first-party error types introduced. My precondition for this is
  that explicit errors are thrown by areas of the code intended for external
  use.

**Efficiency**

- The input CSV is streamed through memory. The program has been tested against
  an input source of 1e9 lines (which would amount to approx. 20GB). See
  `test_large_csv` in `tests/` (disabled by default since it takes a long time).
- Note that `csv` will continue to consume memory until a linefeed is found. I
  did not come across a quick solution for this, and I'd want more than the
  recommended 2-3 hours keyboard time to dig into it. This would definitely need
  to be solved to prevent DoS attacks.
- Client accounts and a transaction cache are held in memory. In a production
  setting these should be stored in a database to prevent potentially unbounded
  memory consumption.
- In concurrent settings (reading from TCP streams, replicated microservices
  etc.) such a database should support appropriate isolation (row-level locking,
  exclusive on writes, shared on reads).

# Commit msg:

One commit to rule them all...

Apologies for the mono-commit. I had strange ideas about not wanting to reveal
how much time I'd spent on this project and it only dawned on my later that this
is not very important and it would have been useful to show the stages of
development.
