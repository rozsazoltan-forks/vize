# @vizejs/musea-mcp-server

MCP (Model Context Protocol) server for Musea design system integration.

## Features

- **AI Integration** - Connect Musea to AI assistants
- **Component Discovery** - Query available components
- **Design Token Access** - Retrieve design system tokens
- **Documentation** - Access component docs via MCP

## Installation

Install `vp` once from the [Vite+ install guide](https://viteplus.dev/guide/install), then add the server to your project:

```bash
vp install -D @vizejs/musea-mcp-server
```

## Usage

### With Claude Desktop

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "musea": {
      "command": "vp",
      "args": ["dlx", "@vizejs/musea-mcp-server", "--project", "/path/to/project"]
    }
  }
}
```

### Standalone

```bash
vp dlx @vizejs/musea-mcp-server --project ./my-vue-app
```

## MCP Tools

- `list_components` - List all components
- `get_component` - Get component details
- `get_variants` - Get component variants
- `get_tokens` - Get design tokens

## License

MIT
