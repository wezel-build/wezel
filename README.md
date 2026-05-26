# Wezel — Your build, always at its best.

Wezel is an open-source build observability toolsuite - think of benchmarking, but for your builds. It makes it easy to figure out when your builds have regressed.

## Getting started

Wezel tracks your build health via *experiments*. An experiment consists of several *steps* (such as running the build, applying patches or running commands). Each step can produce zero or more *outcomes* (artifacts): sizes of your artifacts, profiling info from the compiler, exact timings of your build. Outcomes can then be distilled into a single numerical value (which is called "summarization") that can then be used to track your build health.

An example experiment, straight from our repository is [an experiment measuring artifact size of a release binary](.wezel/experiments/release-build/experiment.toml).
```toml
description = "Measures release-binary size of the wezel CLI"

# Each experiment runs in a fresh copy of your repository, hence we need to run the build first.
# A step is keyed `[step.<tool>.<step_name>]` — `exec` here is the tool, `build-release` is the step name.
# `exec` executes a program on your behalf and produces no outcomes by itself.
[step.exec.build-release]
# Each tool has its own schema for the arguments it accepts. `exec` accepts `cmd`, `env` and `cwd`.
cmd = "cargo build --release --workspace"

# Steps are ran sequentially in source order, so `measure-size` runs after a successful execution of `build-release` step.
# `filesize` produces one outcome per matched file (its size in bytes). It accepts a single argument: `glob`.
[step.filesize.measure-size]
glob = "target/release/wezel"
# Finally, we need to extract a metric value that we can bisect over:
# we want to find an exact commit that causes a regression in the size of target/release/wezel.
summary.wezel-binary-size = { outcome = "target/release/wezel" }
```

Running an experiment is as simple as `wezel experiment run EXPERIMENT_NAME`. All experiments need to live in separate directories of `.wezel/experiments/` subdirectory of your project where the name of subdirectory becomes a name of experiment.

### Setup
Wezel is a CLI that can be installed from GH assets. 

### Tools
Tool implementations are not hard-coded. They are external binaries provided by third parties. Before you run wezel in your project, you need to run `wezel project init` in order to generate a `.wezel/config.toml` file. A `tools` section of `config.toml` defines which tools are available in experiments and how they can be obtained by wezel.
See [.wezel/config.toml](.wezel/config.toml) for reference.
This also means that you can create your own tools to share with the world.

## License

Licensed under [Apache 2.0](LICENSE-APACHE).
