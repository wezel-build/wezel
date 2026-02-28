## Wezel vision doc
The reality:
1. Companies do not care about builds - they care about their product.
2. They do not want to hire build experts or spend time refactoring their build system.
3. When they do spend time on optimizing the build system, they want to see the gains.
4. Those gains are elusive - builds cannot be "refactored". They rot quickly, because the quality of a build graph is neither a primary
  concern nor an easy one to maintain.
5. It is not easy to tell at a glance what the impact of a code change is on the build graph. As engineers, we focus on systems correctness and not on build performance. We do not have necessary tools to care about them, and there is no way to prevent future regressions.


Wezel slides into that reality by helping keep your hand on a pulse. It's a suite of tools for tracking the health of your build. It is out of your way, lightweight and uncompromising. Yet, it will ring the bells the moment
your dev experience regresses. It let's you introspect your code and how it corresponds to the impact on a total build time.


### Measures

The ultimate measure of a build is *time*. Time measurements are not deterministic. They are the ground truth though, so we need to use all the tools at our disposal to correllate a change in build time with the change in code or infrastructure.
We need to combine time measurements (volatile) with the data we can collect from a build graph (non-volatile). An example of non-volatile measurement is `cargo llvm-lines`. It does not change based on which crate you're rebuilding from and such. It depends on the build profile (dev vs release).

Non-volatile measures can be gathered as an asynchronous CI step (since it doesn't matter who runs the build, the data *must* be the same). The volatile measures will be gathered locally. They are dependent on the machine (cores, mostly) and the state of the codebase.

### Wezel's approach
Wezel places emphasis on highlighting the scenarios that get executed the most often. It associates *builds* with their *scenarios* (what code gets built - tests/non tests) and *configurations* (how it is built). 

There are four faces to Wezel:
- Ligthweight agent running locally (Pheromone) - that identifies what code-changes developers make locally. 
- The dashboard (Anthill), showcasing which scenarios get executed the most often. It lets the user make the decision as to which scenarios should be tracked by..
- The backend (Burrow) - the infrastructure beneath Anthill. It ingests events from Pheromone, stores them, and serves data to the dashboard.
- The asynchronous scenario executor (provided by the client) named Forager. It runs the scenarios and gathers the measures (both volatile and non-volatile ones). 

### Burrow
Burrow is Anthill's backend. It receives events flushed by Pheromone, persists them, and exposes the data Anthill needs to render scenarios, configurations, and their measures.

### Pheromone
Pheromone is an agent running locally. It consists of a single binary (pheromone_cli) that is invoked via precmd hooks in the shell. The cli delegates to the build-system-specific processes named `pheromone-<build system>` such as `pheromone-cargo` for Rust. The build system-specific process is responsible for identifying the scenario being executed and reporting it back to the pheromone_cli.
All events are dumped into ~/.wezel/events/.json. As a post-cmd hook (in the background), wezel will flush the events to the currently configured Anthill instance.

pheromone_cli is thus responsible for:
- shell handling (precmd and postcmd hooks)
- Alias normalization (cargo build and cargo b are the same)
- Flushing the events to Anthill

#### Custom toolchains
Build systems often circumvent the shell; for example, rustup may end up invoking the cargo binary directly. In such cases one can set up a custom toolchain that invokes pheromone-cargo instead of cargo (busybox-style). This way, the events will be captured as well. The same applies to other build systems. Wezel will provide a set of instructions for setting up such custom toolchains for the most popular build systems.
