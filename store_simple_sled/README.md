# willow-store-simple-sled

Simple persistent storage for Willow data.

- Implements [`willow_data_model::Store`].
- _Simple_, hence it has a straightforward implementation without the use of
  fancy data structures.
- Uses [sled](https://docs.rs/sled/latest/sled/) under the hood.

```rs
let db = sled::open("my_db").unwrap();
let namespace = willow_25::NamespaceId25::new_communal();

let store = StoreSimpleSled::new(namespace, db).unwrap();
```

# Performance considerations

- Read and write performance should be adequate.
- Loads entire payloads into memory all at once.
