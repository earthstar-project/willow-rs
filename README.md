# willow-rs

> Protocols for synchronisable data stores. The best parts? Fine-grained
> permissions, a keen approach to privacy, destructive edits, and a dainty
> bandwidth and memory footprint.

_Currently_, this repo provides Rust implementations of:

- [Meadowcap](https://willowprotocol.org/specs/meadowcap/index.html#meadowcap),
  a capability system adaptable to local needs,
- Everything in the
  [Willow Data Model](https://willowprotocol.org/specs/data-model/index.html#data_model)
  (parameters, paths, entries, groupings, encodings) _except_ for the
  all-important
  [store](https://willowprotocol.org/specs/data-model/index.html#store).

_Eventually_, this repo will house Rust implementations of:

- The aforementioned all-important
  [store](https://willowprotocol.org/specs/data-model/index.html#store),
- [Willow Sideloading protocol](https://willowprotocol.org/specs/sideloading/index.html#sideloading),
  eventually consistent data delivered by any means possible,
- and
  [Willow General Purpose Sync Protocol](https://willowprotocol.org/specs/sync/index.html#sync),
  private and efficient synchronisation of Willow stores.

We welcome contributions! If you're looking for contribution ideas, please see
the repo's issues, milestones, and projects.

## See also

- [Willow website](https://willowprotocol.org)
- [willow-js](https://github.com/earthstar-project/willow-js) - TypeScript
  implementation of Willow Data Model, Sideloading, and General Purpose Sync
  protocol.
- [meadowcap-js](https://github.com/earthstar-project/meadowcap-js) - TypeScript
  implementation of Meadowcap

---

This project was funded through the [NGI0 Core](https://nlnet.nl/core) Fund, a
fund established by [NLnet](https://nlnet.nl/) with financial support from the
European Commission's [Next Generation Internet](https://ngi.eu/) programme,
under the aegis of
[DG Communications Networks, Content and Technology](https://commission.europa.eu/about-european-commission/departments-and-executive-agencies/communications-networks-content-and-technology_en)
under grant agreement No
[101092990](https://cordis.europa.eu/project/id/101092990).
