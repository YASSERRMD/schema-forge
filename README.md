# Schema-Forge

An intelligent CLI-based database agent that provides an interactive REPL for querying databases using natural language, powered by LLM providers.

## Features

- **Multi-Database Support**: PostgreSQL, MySQL, SQLite, MSSQL
- **8 LLM Providers**: Anthropic Claude, OpenAI GPT, Groq, Cohere, xAI, Minimax, Qwen, z.ai
- **Natural Language Queries**: Ask questions in plain English, get SQL results
- **Automatic Schema Indexing**: Introspects and understands your database structure
- **Interactive REPL**: rustyline-powered CLI with command history and tab completion
- **Smart SQL Generation**: LLM-powered natural language to SQL translation
- **Fast & Reliable**: Built with Rust, featuring retry logic and exponential backoff

## Installation

### Prerequisites

- Rust 1.70+ and Cargo
- A database (PostgreSQL, MySQL, SQLite, or MSSQL)
- API key for your preferred LLM provider

### Build from Source

```bash
# Clone the repository
git clone https://github.com/YASSERRMD/schema-forge.git
cd schema-forge

# Build the project
cargo build --release

# The binary will be at target/release/schema-forge
```

### Install via Cargo (Coming Soon)

```bash
cargo install schema-forge
```

## Quick Start

```bash
# Run Schema-Forge
cargo run --release

# Connect to your database
> /connect postgresql://user:password@localhost/mydb

# Index the database schema
> /index

# Set your LLM provider
> /config anthropic sk-ant-your-key-here

# Ask a question in natural language
> Show me all users who signed up in the last 30 days

# Or get SQL directly
> How many users do we have?
```

## Commands

### Database Commands

| Command | Description | Example |
|---------|-------------|---------|
| `/connect <url>` | Connect to a database | `/connect postgresql://localhost/mydb` |
| `/index` | Index the database schema | `/index` |

### Configuration Commands

| Command | Description | Example |
|---------|-------------|---------|
| `/config <provider> <key>` | Set API key for LLM provider | `/config openai sk-...` |

### Session Commands

| Command | Description |
|---------|-------------|
| `/clear` | Clear chat context |
| `/help` | Show help message |
| `/quit` or `/exit` | Exit Schema-Forge |

## Supported Databases

### PostgreSQL
```bash
/connect postgresql://user:password@localhost:5432/mydb
/connect postgres://user:password@localhost/mydb
```

### MySQL
```bash
/connect mysql://user:password@localhost:3306/mydb
```

### SQLite
```bash
/connect sqlite://path/to/database.db
```

### MSSQL
```bash
/connect mssql://user:password@localhost:1433/mydb
/connect sqlserver://user:password@localhost:1433/mydb
```

## Supported LLM Providers

### Anthropic (Claude)
```bash
/config anthropic sk-ant-your-api-key-here
```
Models: `claude-3-5-sonnet-20241022` (default), `claude-3-opus`

### OpenAI (GPT)
```bash
/config openai sk-your-api-key-here
```
Models: `gpt-4o-mini` (default), `gpt-4`, `gpt-3.5-turbo`

### Groq (Llama)
```bash
/config groq gsk-your-api-key-here
```
Models: `llama3-70b-8192` (default), `mixtral-8x7b-32768`

### Cohere
```bash
/config cohere your-api-key-here
```
Models: `command-r-plus` (default), `command-r`

### xAI (Grok)
```bash
/config xai sk-your-api-key-here
```
Models: `grok-beta` (default), `grok-2`

### Minimax
```bash
/config minimax your-api-key-here
```
Models: `abab6.5s-chat` (default), `abab5.5-chat`

### Qwen (Alibaba Cloud)
```bash
/config qwen sk-your-api-key-here
```
Models: `qwen-turbo` (default), `qwen-max`

### z.ai
```bash
/config z.ai your-api-key-here
```
Models: `z-pro-v1` (default), `z-ultra-v2`

## Usage Examples

### Query Your Database

```
> Show me all users older than 25
[Processes with LLM and executes SQL]

> Count users by country
[Returns aggregated results]

> Find the top 10 customers by revenue
[Returns ranked results]
```

### Get SQL Only

Schema-Forge can also generate SQL queries for you to review:

```
> Convert to SQL: users who registered in 2024
SELECT * FROM users WHERE YEAR(created_at) = 2024;
```

### Schema Information

The `/index` command scans your database and provides LLMs with complete schema context:

- Table names and types (tables vs views)
- Column names, data types, and nullability
- Primary keys and unique constraints
- Foreign key relationships
- Default values

## Architecture

Schema-Forge is built with a modular architecture:

- **`database/`**: Database connections, schema indexing, caching
- **`llm/`**: LLM provider abstractions and implementations
- **`cli/`**: REPL, command parsing, and user interaction
- **`config/`**: Application state and configuration management
- **`error/`**: Comprehensive error handling

## Development

### Run Tests

```bash
# Run all tests
cargo test

# Run specific module tests
cargo test database
cargo test llm
cargo test cli

# Run with output
cargo test -- --nocapture --test-threads=1
```

### Code Quality

```bash
# Format code
cargo fmt

# Check formatting
cargo fmt --check

# Run linter
cargo clippy -- -D warnings
```

### Build Debug

```bash
cargo build
cargo run
```

### Build Release

```bash
cargo build --release
./target/release/schema-forge
```

## Project Status

- Phase 1: Project scaffolding and core data structures (COMPLETED)
- Phase 2: DatabaseManager and schema indexing system (COMPLETED)
- Phase 3: LLM integration with trait-based provider system (COMPLETED)
- Phase 4: CLI REPL with rustyline and command handling (COMPLETED)
- Phase 5: Integration testing and refinement (COMPLETED)

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'feat: add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Acknowledgments

- Built with [Rust](https://www.rust-lang.org/)
- LLM integration powered by [Anthropic](https://www.anthropic.com/), [OpenAI](https://openai.com/), [Groq](https://groq.com/), [Cohere](https://cohere.com/), [xAI](https://x.ai/), [Minimax](https://www.minimaxi.com/), [Alibaba Qwen](https://tongyi.aliyun.com/), and [z.ai](https://z.ai/)
- Database access via [sqlx](https://github.com/launchbadge/sqlx) and [tiberius](https://github.com/prisma/tiberius)
- CLI powered by [rustyline](https://github.com/kkawakam/rustyline)

## Author

Built by [YASSERRMD](https://github.com/YASSERRMD)

---

Schema-Forge: Query your database with natural language.
