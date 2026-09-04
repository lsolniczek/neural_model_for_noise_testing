# P-01 continuous integration

The NMM workflow has two independent checks:

- `python-and-baseline` is public and runs for every push and pull request. It
  discovers the complete Python test tree and verifies the integrity of the
  frozen Stage 1 artifact.
- `rust-canonical` runs on the canonical macOS ARM64 environment. It resolves
  the lockfile, tests all Rust targets, and explicitly exercises export replay.

The Rust job fetches `noise_generator_core` from the exact Git revision in
`Cargo.lock`. It never follows a branch or a mutable tag.

## Configure private DSP access

1. Generate a dedicated SSH key pair for CI. Do not reuse a developer key.
2. In `lsolniczek/noise_generator_dsp`, add the public key under
   **Settings → Deploy keys**. Leave **Allow write access** disabled.
3. In the NMM repository, add the private key as an Actions secret named
   `DSP_DEPLOY_KEY`.
4. Run the workflow manually once and confirm that `Resolve the locked
   dependency graph` fetches the pinned DSP commit.

The workflow fails before Cargo runs when the secret is missing. Pull requests
from forks do not receive repository secrets, so they run the public Python and
artifact checks while skipping `rust-canonical`. Pushes and same-repository
pull requests must run both jobs.

For branch protection, require `Python tests and historical baseline` and
`Rust canonical (macOS ARM64)` on branches where changes are merged from the
same repository. Never replace the pull-request workflow with
`pull_request_target`, because that would expose a privileged context to code
from the pull request.
