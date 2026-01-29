# nada-cli: Command-line interface for Nada

Currently, this interface supports two kinds of operations:

* `nada kubeconfig` - sets up Kubernetes cluster configuration and credentials, like the `nais` tool but for Nada; and
* `nada jita` - requests just-in-time access to sensitive resources in Nada GCP projects.

Run `nada --help` to show all possible sub-commands and their syntax.

For sub-command help, you may use `nada <SUBCOMMAND> --help`.

The command-line interface relies heavily on `gcloud`. Before using the Nada CLI, please authenticate using `gcloud` and
make sure to update your _application default credentials_. i.e.,

```shell
gcloud auth login --update-adc --force
```

## Installing

### Prerequisites

You must install `kubectl`, `kubectx`, `gcloud`, and `gke-gcloud-auth-plugin`.

Refer to the [Nais Command-line access documentation](https://doc.nais.io/operate/how-to/command-line-access) for setup
instructions.

Installing `kubelogin` is not required for the Nada command-line interface.

### MacOS/Linux 

```
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/navikt/nada-cli/releases/download/v0.1.0/nada-cli-installer.sh | sh
```

### Windows
```
powershell -ExecutionPolicy Bypass -c "irm https://github.com/navikt/nada-cli/releases/download/v0.1.0/nada-cli-installer.ps1 | iex"
```

### Manual Installation steps

First, install Rustup, the Rust toolchain manager. Then, install the Rust toolchains for your architecture:

```shell
brew install rustup
rustup update
```

Next, clone this repository and install the binary:

```shell
git clone git@github.com:navikt/nada-cli
cd nada-cli
cargo install --path .
```

Any binaries install with Cargo will be placed in `~/.cargo/bin`. Make sure your `$PATH` environment variable includes
Cargo's bin directory.

## Releasing

Create a new release by bumping the version, committing the changes, and tagging the commit:

```sh
scripts/release.sh patch
```
The script updates Cargo.toml and `README.md` with the new version number, creates a git commit and tag, and pushes them to main after confirmation.
