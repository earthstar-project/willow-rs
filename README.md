# willow-rs

> Protocols for synchronisable data stores. The best parts? Fine-grained
> permissions, a keen approach to privacy, destructive edits, and a dainty
> bandwidth and memory footprint.

## See also

- [Willow website](https://willowprotocol.org)

## Fuzz tests

This repository has many fuzz tests. To use `cargo fuzz` commands, you must
first make `fuzz` the working directory so that the nightly compiler (on which
cargo-fuzz relies) is used for compiling the tests.

```
cd fuzz
cargo fuzz run <test_name_here>
```

There is also a command for running all the fuzz tests sequentially:

```
cd fuzz
./run_all.sh -- -max_total_time=<number_of_seconds>
```

---

This project was funded through the [NGI0 Core](https://nlnet.nl/core) Fund, a
fund established by [NLnet](https://nlnet.nl/) with financial support from the
European Commission's [Next Generation Internet](https://ngi.eu/) programme,
under the aegis of
[DG Communications Networks, Content and Technology](https://commission.europa.eu/about-european-commission/departments-and-executive-agencies/communications-networks-content-and-technology_en)
under grant agreement No
[101092990](https://cordis.europa.eu/project/id/101092990).
