# CodeForFree

A free, open-source, local-first coding chat workspace. The server is written in Rust and the UI runs in your browser. This is an original implementation, not a copy of CC Switch.

## Run on Linux

Requires Rust and an OpenAI-compatible model endpoint. Ollama is the default:

```sh
ollama pull llama3.2
ollama serve
cargo run
```

Open http://127.0.0.1:3000, create a project, and send a message. If Ollama is already running, do not start it a second time.

To use another compatible endpoint, set `MODEL_BASE_URL`, `MODEL_NAME`, and optionally `MODEL_API_KEY` in the server environment. For example:

```sh
MODEL_BASE_URL=http://127.0.0.1:11434/v1 MODEL_NAME=llama3.2 cargo run
```

The server binds to `127.0.0.1:3000` by default. `CODEFORFREE_BIND` changes the address; `CODEFORFREE_DB` changes the SQLite file path. The app has no authentication, so do not expose it to a public network.

## Current scope

Projects and chat history persist in SQLite. Ask, Plan, and Code modes alter the assistant instructions; Code mode currently suggests edits and does not execute tools or modify files. The configured model endpoint is the only provider. Account sign-in, quotas, failover, cross-device sync, and a tool-running coding agent are future work. Credential rotation to bypass consumer usage limits is outside the project's scope.

## Roadmap

1. Workspace file browser and explicit read/edit/terminal permissions.
2. A reliable agent loop with diffs, approvals, and tool logs.
3. Provider adapters for documented APIs, with per-provider availability and permitted failover.
4. Encrypted cross-device sync of project history and settings, excluding credentials by default.
5. Tests, packaging, contributor docs, and a public GitHub release.

The application itself has no paid tier. Third-party models or hosted APIs may charge independently.
