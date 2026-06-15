# `tracing-reload`

`tracing-reload` is (mostly) a replacement for `tracing_subscriber::reload::Layer`.

It provides a layer that should serve the same purpose but work with all layers including those
depending on the downcasting mechanism, and it should not panic. Or at least panic much less.

## Compatibility

One thing that this crate's layer misses that the original has, is `Handle::modify` which lets users
modify the layers in-place. That is impossible to achieve while allowing downcasting. The only way
to update the layer is to create a new one to replace the old one.

For example in the `reload` module in `tracing-subscriber, there is the following example:

```rust
use tracing_subscriber::{filter, fmt, reload, prelude::*};
let filter = filter::LevelFilter::WARN;
let (filter, reload_handle) = reload::Layer::new(filter);
tracing_subscriber::registry()
  .with(filter)
  .with(fmt::Layer::default())
  .init();
info!("This will be ignored");
reload_handle.modify(|filter| *filter = filter::LevelFilter::INFO);
info!("This will be logged");
```

This layer works similarly, but instead of `modify`, one has to call either `reload` or
`reload_with`:

```rust
use tracing_subscriber::{filter, fmt, prelude::*};
let filter = filter::LevelFilter::WARN;
let (filter, reload_handle) = tracing_reload::Layer::new(filter);
tracing_subscriber::registry()
  .with(filter)
  .with(fmt::Layer::default())
  .init();
info!("This will be ignored");
reload_handle.reload(filter::LevelFilter::INFO);
info!("This will be logged");
```

## Supported Rust Versions

`tracing-reload` is built against the latest stable release. The minimum supported version is 1.85.
The current version is not guaranteed to build on Rust versions earlier than the minimum supported
version.

`tracing-reload` follows the same compiler support policies as the Tokio project. The current
stable Rust compiler and the three most recent minor versions before it will always be supported.
For example, if the current stable compiler version is 1.69, the minimum supported version will not
be increased past 1.66, three minor versions prior. Increasing the minimum supported compiler
version is not considered a semver breaking change as long as doing so complies with this policy.

That said, I'll do my best to keep the minimum supported version at least at whatever uses current
Debian stable.

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
`tracing-reload` by you, shall be licensed as MIT, without any additional terms or conditions.
