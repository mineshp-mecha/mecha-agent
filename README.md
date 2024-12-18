# mecha-agent

Mecha Agent is the service running on device to manage the connection with Mecha Services includes features for provisioning, messaging and telemetry. Check the [docs](https://docs.mecha.so) for more information.

## Dependencies

Rust: `1.70 or above`
## Running the mecha-agent on local

1. Clone this repository

```sh
$ git clone https://github.com/mecha-org/mecha-agent
```

2. Create your settings.yml using the `settings.yml.example` provided, update the defaults as necessary

```sh
$ cd mecha-agent
$ cp settings.yml.example settings.yml
```

3. Run the `mecha-agent` using cargo run

```sh
$ cargo run -- -s ./settings.yml
```

4. To generate the release build

```sh
$ cargo build --release
$ ./target/release/mecha_agent_server -s ./settings.yml
```

## Running via Docker

1. Ensure you have `settings.yml` in the repository root directory generated with the required settings for your docker.

2. Build the docker image using the `Dockerfile` provided in the root directory

```sh
$ docker build -t mecha-org/mecha-agent .
```

3. Run the docker image, with the port exposed in your settings.yml for the grpc server

```sh
$ docker run -p 3001:3001 mecha-org/mecha-agent
```

## Commands

### Start

Starts the agent, but only if it is provisioned.

```bash
# If machine found
$ mecha-agent start -s ./settings.yml
> Starting Mecha agent
> Machine Name: mars-machine
> Machine ID: ___
> Mecha agent started successful 
```


#### Options
    -s ./settings.yml: Specifies the path to the settings file.
    --server: Enables GRPC server mode.
#### Notes
    GRPC: By default, GRPC does not start unless --server is used.
    Server Mode: If the --server flag is added, GRPC will be enabled.

### Setup
Runs a provisioning flow via CLI.
```bash
$ mecha-agent setup
> Generating code ... > XYZ-123
> Waiting for provisioning ...
> Machine Name: mars-machine
> Machine ID: _____
> Machine setup complete
```


### Whoami

```bash
$ mecha-agent whoami
> Machine details fetched
ID: ___
Name: ___
Alias: ___
Certificate Serial Number: ___
Certificate Fingerprint: ___

```

### Reset
```bash
$ mecha-agent reset
> Are you sure you want to reset the agent (Name: mars-machine) [Y/N] - ?
> Agent reset successful
```