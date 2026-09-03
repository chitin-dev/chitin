# chitin

<p align="center" name="markdown">
  <img src="assets/logo.svg" alt="chitin-dev" width="100" />
</p>

<p align="center">
  <a href="https://chitin-ide.dev">chitin-ide.dev</a> · <a href="#install">install</a> · <a href="https://chitin-ide.dev/docs/quick-start/">quick start</a> · <a href="https://chitin-ide.dev/docs/">docs</a>
</p>

---

## ✨ Features

**Chitin** is an agent-native computational chemistry and bioinformatics integrated
development suite.

See [ROADMAP.md](ROADMAP.md) for the project roadmap.

## ❄️ Development with Nix

The repository provides reproducible development shells for Rust/WGPU work,
documentation, and F* verification. Enter the default shell and run the same
tasks used by CI:

```bash
env -u LD_LIBRARY_PATH nix develop
just ci
```

Use the specialised shells when needed:

```bash
env -u LD_LIBRARY_PATH nix develop .#docs
env -u LD_LIBRARY_PATH nix develop .#formal
```

The `LD_LIBRARY_PATH` cleanup is needed only when a host shell globally sets it;
mixing host libraries with Nix libraries can cause glibc symbol errors.
