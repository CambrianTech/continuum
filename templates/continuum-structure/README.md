# Continuum Configuration

This directory contains Continuum AI configuration and data.

## Structure

```
.continuum/
├── config.env              # Team shared configuration
├── shared/                 # Team shared data (committed to git)
│   └── models.json         # Approved models and limits
├── users/                  # User-specific data (gitignored)
│   └── [username]/
│       ├── config.env      # Personal API keys & settings
│       ├── conversation-history.jsonl  # Private chat logs
│       └── costs.json      # Personal cost tracking
└── .gitignore             # Keeps user data private
```

## Setup

1. **Copy user template**: `cp users/EXAMPLE_USER/config.env users/[your-username]/config.env`
2. **Add your API keys**: Edit `users/[your-username]/config.env` with your actual keys
3. **Team settings**: Edit `config.env` and `shared/models.json` for team preferences

## Privacy

- `users/` directory is automatically gitignored
- Your API keys and chat logs stay private
- Only team configuration is shared via git