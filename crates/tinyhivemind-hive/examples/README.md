# Examples

| file/dir | what it does |
| --- | --- |
| [`hive.rs`](hive.rs) | Runs one deliberation episode with scripted, deterministic agents and prints the trace turn by turn — the fastest way to see the protocol's shape without a model. `cargo run -p tinyhivemind-hive --example hive`. |
| [`bench/`](bench/README.md) | The simulation and benchmark harness: scores deliberation against the responder ladder and a matched-budget vote across thousands of reproducible synthetic rooms, sweeps the episode policy, and can drive a real agent CLI or HTTP endpoint. See its own README and companion docs. |
