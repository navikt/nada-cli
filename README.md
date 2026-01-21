# nada-cli: Command-line interface for Nada

Currently, this interface supports two kinds of operations:

* `nada kubeconfig` - sets up Kubernetes cluster configuration and credentials, like the `nais` tool but for Nada; and
* `nada jita` - requests just-in-time access to sensitive resources in Nada GCP projects.

Run `nada --help` to show all possible sub-commands and their syntax.

For sub-command help, you may use `nada <SUBCOMMAND> --help`.

## Installing

### Prerequisites

You must install `kubectl`, `gcloud`, and `gke-gcloud-auth-plugin`.

Refer to the [Nais Command-line access documentation](https://doc.nais.io/operate/how-to/command-line-access) for setup
instructions.

Installing `kubelogin` is not required for the Nada command-line interface.

### Installation steps

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