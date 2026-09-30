use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub icon: String,
    pub command: String,
    pub args: Vec<String>,
    pub env_keys: Vec<String>,
    pub enabled_by_default: bool,
    #[serde(default)]
    pub is_custom: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub icon: String,
    pub command: String,
    pub args: Vec<String>,
    pub env_keys: Vec<String>,
    pub enabled: bool,
    pub is_custom: bool,
    pub env_configured: bool,
    pub assigned_bot_ids: Vec<String>,
    pub tools_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTestResult {
    pub success: bool,
    pub server_id: String,
    pub message: String,
    pub latency_ms: u64,
    pub tools: Vec<crate::client::McpTool>,
}

pub fn category_of(id: &str) -> &'static str {
    match id {
        "github" | "gitlab" | "bitbucket" | "git" | "filesystem" | "docker" | "kubernetes" | "postman" | "npm" | "pypi"
        | "sentry" | "linear" | "jira" | "azure_devops" | "github_actions" | "railway" | "render" | "flyio" | "argocd" | "posthog" | "sonarqube" => "Development & Coding",
        
        "postgres" | "mysql" | "sqlite" | "mongodb" | "redis" | "supabase" | "firebase" | "neon" | "clickhouse" | "snowflake"
        | "bigquery" | "planetscale" | "couchdb" | "meilisearch" | "duckdb" | "elasticsearch" | "opensearch" | "neo4j" | "surrealdb" | "timescaledb" => "Databases",
        
        "fetch" | "brave_search" | "google_search" | "tavily" | "exa" | "firecrawl" | "puppeteer" | "playwright" | "browserbase"
        | "perplexity" | "serpapi" | "wolfram_alpha" | "arxiv" | "wikipedia" | "scrapfly" | "brightdata" => "Web & Research",
        
        "aws" | "cloudflare" | "gcloud" | "azure" | "vercel" | "terraform" | "pulumi" | "hetzner" | "digitalocean" => "Cloud & Infrastructure",
        
        "gdrive" | "gcalendar" | "gsheets" | "gdocs" | "notion" | "slack" | "discord" | "teams" | "m365" | "todoist"
        | "asana" | "trello" | "airtable" | "obsidian" | "confluence" | "clickup" | "basecamp" | "readwise" => "Productivity",
        
        "figma" | "canva" | "blender" | "unity" | "unreal" | "midjourney" => "Design & Creative",
        
        "openai" | "anthropic" | "huggingface" | "replicate" | "langchain" | "pinecone" | "qdrant" | "weaviate" | "chroma" | "milvus" => "AI / ML",
        
        "stripe" | "shopify" | "salesforce" | "hubspot" | "paypal" | "quickbooks" | "zendesk" => "Business / Commerce",
        
        "yfinance" | "alpha_vantage" | "coingecko" | "etherscan" | "alchemy" => "Finance & Web3",
        
        "telegram" | "whatsapp" | "twitter" | "reddit" | "resend" | "sendgrid" | "twilio" => "Social & Messaging",
        
        "datadog" | "grafana" | "semgrep" | "snyk" | "vault" | "onepassword" | "bitwarden" | "tailscale" | "shodan" | "virustotal" => "Security / Observability",
        
        "home_assistant" | "mqtt" => "Smart Home & IoT",
        
        "shell" | "ssh" | "clipboard" | "audio" => "Local Computer",
        
        _ => "Other",
    }
}

pub fn all_servers() -> Vec<McpServerConfig> {
    let mut v = Vec::new();
    let mut add = |id: &str, name: &str, desc: &str, icon: &str, cmd: &str, args: &[&str], env: &[&str]| {
        v.push(McpServerConfig {
            id: id.to_string(),
            name: name.to_string(),
            description: desc.to_string(),
            category: category_of(id).to_string(),
            icon: icon.to_string(),
            command: cmd.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            env_keys: env.iter().map(|s| s.to_string()).collect(),
            enabled_by_default: matches!(id, "filesystem" | "git" | "sqlite" | "fetch" | "shell" | "docker" | "duckdb" | "wikipedia"),
            is_custom: false,
        });
    };

    // Development & Coding (21)
    add("github", "GitHub MCP", "Repositories, issues, PRs, commits, code search", "🐙", "npx", &["-y", "@modelcontextprotocol/server-github"], &["GITHUB_PERSONAL_ACCESS_TOKEN"]);
    add("gitlab", "GitLab MCP", "Projects, issues, merge requests, CI pipelines", "🦊", "npx", &["-y", "@modelcontextprotocol/server-gitlab"], &["GITLAB_PERSONAL_ACCESS_TOKEN"]);
    add("bitbucket", "Bitbucket MCP", "Repositories, PRs, workspaces, branch management", "🪣", "npx", &["-y", "mcp-remote", "https://mcp.bitbucket.org/sse"], &["BITBUCKET_TOKEN"]);
    add("sentry", "Sentry MCP", "Real-time error alerts, issues, stack trace debugging", "🚨", "npx", &["-y", "@sentry/mcp-server"], &["SENTRY_AUTH_TOKEN"]);
    add("linear", "Linear MCP", "Issues, projects, cycles, triage workflows", "📐", "npx", &["-y", "mcp-remote", "https://mcp.linear.app/sse"], &["LINEAR_API_KEY"]);
    add("jira", "Jira MCP", "Atlassian Jira project management and sprint tracking", "🗂️", "npx", &["-y", "mcp-remote", "https://mcp.atlassian.com/v1/sse"], &["JIRA_API_TOKEN"]);
    add("azure_devops", "Azure DevOps MCP", "Azure Repos, Boards, Pipelines, Artifacts", "🔷", "npx", &["-y", "@azure-devops/mcp"], &["AZURE_DEVOPS_PAT"]);
    add("github_actions", "GitHub Actions MCP", "Trigger workflows, inspect CI logs, retry jobs", "⚡", "npx", &["-y", "@modelcontextprotocol/server-github-actions"], &["GITHUB_PERSONAL_ACCESS_TOKEN"]);
    add("railway", "Railway MCP", "Full-stack cloud deployments, environments, variables", "🚂", "npx", &["-y", "mcp-railway"], &["RAILWAY_API_TOKEN"]);
    add("render", "Render MCP", "Deploy web services, databases, static sites, cron jobs", "🚀", "npx", &["-y", "mcp-render"], &["RENDER_API_KEY"]);
    add("flyio", "Fly.io MCP", "Global micro-VM containers, scale apps, regions", "🎈", "npx", &["-y", "mcp-flyio"], &["FLY_API_TOKEN"]);
    add("argocd", "ArgoCD GitOps MCP", "Kubernetes continuous delivery, sync apps, status", "🐙", "npx", &["-y", "mcp-argocd"], &["ARGOCD_AUTH_TOKEN"]);
    add("posthog", "PostHog MCP", "Product analytics, feature flags, session replays, funnels", "🦔", "npx", &["-y", "mcp-posthog"], &["POSTHOG_API_KEY"]);
    add("sonarqube", "SonarQube MCP", "Code quality gates, SAST security analysis, debt metrics", "🔍", "npx", &["-y", "mcp-sonarqube"], &["SONAR_TOKEN"]);
    add("filesystem", "Filesystem MCP", "Direct local file inspection, reading, writing", "📁", "npx", &["-y", "@modelcontextprotocol/server-filesystem", "/tmp"], &[]);
    add("git", "Git MCP", "Local Git branches, log, diff, commit, rebase", "🌿", "npx", &["-y", "@modelcontextprotocol/server-git"], &[]);
    add("docker", "Docker MCP", "Container management, image building, runtime exec", "🐳", "npx", &["-y", "mcp-remote", "https://mcp.docker.com/sse"], &[]);
    add("kubernetes", "Kubernetes MCP", "Cluster workloads, pod inspection, Helm deployments", "☸️", "npx", &["-y", "mcp-remote", "https://mcp.kubernetes.io/sse"], &["KUBECONFIG"]);
    add("postman", "Postman MCP", "API collections, environments, and mock runs", "📮", "npx", &["-y", "@postman/mcp-server"], &["POSTMAN_API_KEY"]);
    add("npm", "NPM Registry MCP", "Search packages, inspect versions, verify dependencies", "📦", "npx", &["-y", "@modelcontextprotocol/server-npm"], &[]);
    add("pypi", "PyPI Registry MCP", "Python package index search and metadata inspection", "🐍", "npx", &["-y", "mcp-pypi"], &[]);

    // Databases (20)
    add("postgres", "PostgreSQL MCP", "Read/write SQL queries, schema inspection, tables", "🐘", "npx", &["-y", "@modelcontextprotocol/server-postgres"], &["POSTGRES_CONNECTION_STRING"]);
    add("mysql", "MySQL MCP", "MySQL and MariaDB queries, transactions, tables", "🐬", "npx", &["-y", "@benborla/mcp-server-mysql"], &["MYSQL_DSN"]);
    add("sqlite", "SQLite MCP", "Local SQLite database file querying and schema", "🗃️", "npx", &["-y", "mcp-remote", "https://mcp.sqlite.dev/sse"], &[]);
    add("duckdb", "DuckDB MCP", "Ultra-fast in-process analytical SQL for Parquet, CSV, Arrow", "🦆", "npx", &["-y", "@modelcontextprotocol/server-duckdb"], &[]);
    add("mongodb", "MongoDB MCP", "Document collections, aggregation pipeline, indexes", "🍃", "npx", &["-y", "mcp-remote", "https://mcp.mongodb.com/sse"], &["MONGODB_URI"]);
    add("redis", "Redis MCP", "In-memory key-value cache, pub/sub, queues", "🔴", "npx", &["-y", "@gongrz/mcp-server-redis"], &["REDIS_URL"]);
    add("supabase", "Supabase MCP", "Postgres, Auth, Storage, Edge Functions", "⚡", "npx", &["-y", "@supabase/mcp-server-supabase"], &["SUPABASE_ACCESS_TOKEN"]);
    add("firebase", "Firebase MCP", "Firestore, Realtime Database, Cloud Auth", "🔥", "npx", &["-y", "@gongrz/mcp-server-firebase"], &["FIREBASE_TOKEN"]);
    add("neon", "Neon MCP", "Neon serverless Postgres branching and queries", "🌙", "npx", &["-y", "@neondatabase/mcp-server-neon"], &["NEON_API_KEY"]);
    add("clickhouse", "ClickHouse MCP", "Fast analytical database queries and time-series", "📊", "npx", &["-y", "@clickhouse/mcp-server"], &["CLICKHOUSE_DSN"]);
    add("snowflake", "Snowflake MCP", "Snowflake Data Cloud queries and data warehouses", "❄️", "npx", &["-y", "mcp-remote", "https://mcp.snowflake.com/sse"], &["SNOWFLAKE_URI"]);
    add("bigquery", "BigQuery MCP", "Google BigQuery enterprise analytics and datasets", "📈", "npx", &["-y", "@modelcontextprotocol/server-bigquery"], &["BIGQUERY_CREDENTIALS"]);
    add("planetscale", "PlanetScale MCP", "Serverless MySQL branching and migrations", "🪐", "npx", &["-y", "mcp-planetscale"], &["PLANETSCALE_SERVICE_TOKEN"]);
    add("couchdb", "CouchDB MCP", "Apache CouchDB JSON documents and views", "🛋️", "npx", &["-y", "mcp-couchdb"], &["COUCHDB_URL"]);
    add("elasticsearch", "Elasticsearch MCP", "Distributed enterprise full-text search and aggregations", "🔍", "npx", &["-y", "@elastic/mcp-server-elasticsearch"], &["ELASTICSEARCH_URL", "ELASTICSEARCH_API_KEY"]);
    add("opensearch", "OpenSearch MCP", "OpenSearch search clusters, observability, vectors", "🔎", "npx", &["-y", "mcp-opensearch"], &["OPENSEARCH_URL"]);
    add("neo4j", "Neo4j Graph MCP", "Graph database Cypher queries and relationship traversal", "🕸️", "npx", &["-y", "@neo4j/mcp-server-neo4j"], &["NEO4J_URI", "NEO4J_PASSWORD"]);
    add("surrealdb", "SurrealDB MCP", "Multi-model database queries for document, graph, relational", "⚡", "npx", &["-y", "mcp-surrealdb"], &["SURREAL_URL"]);
    add("timescaledb", "TimescaleDB MCP", "Time-series hypertables and analytics on PostgreSQL", "⏱️", "npx", &["-y", "mcp-timescaledb"], &["TIMESCALE_CONNECTION_STRING"]);
    add("meilisearch", "Meilisearch MCP", "Lightning-fast typo-tolerant full text search", "🔍", "npx", &["-y", "@meilisearch/mcp-server"], &["MEILISEARCH_KEY"]);

    // Web & Research (16)
    add("fetch", "Fetch MCP", "Raw web page content retrieval and HTML parsing", "🌐", "npx", &["-y", "@modelcontextprotocol/server-fetch"], &[]);
    add("brave_search", "Brave Search MCP", "Privacy-first real-time global web search", "🦁", "npx", &["-y", "@modelcontextprotocol/server-brave-search"], &["BRAVE_API_KEY"]);
    add("google_search", "Google Search MCP", "Google Search API for real-time web results", "🔍", "npx", &["-y", "mcp-remote", "https://mcp.googleapis.com/sse"], &["GOOGLE_API_KEY"]);
    add("tavily", "Tavily MCP", "AI-optimized search engineered for LLM agents", "🔎", "npx", &["-y", "mcp-remote", "https://mcp.tavily.com/sse"], &["TAVILY_API_KEY"]);
    add("exa", "Exa MCP", "Neural semantic web search and link embeddings", "✨", "npx", &["-y", "exa-mcp-server"], &["EXA_API_KEY"]);
    add("firecrawl", "Firecrawl MCP", "Crawl full domains and convert pages to clean markdown", "🔥", "npx", &["-y", "firecrawl-mcp"], &["FIRECRAWL_API_KEY"]);
    add("scrapfly", "Scrapfly Anti-Bot Scraper", "Bypass anti-bot protections, rotate residential IPs", "🪰", "npx", &["-y", "mcp-scrapfly"], &["SCRAPFLY_API_KEY"]);
    add("brightdata", "Bright Data Web Unlocker", "Automated CAPTCHA bypass, residential browser proxies", "🌐", "npx", &["-y", "mcp-brightdata"], &["BRIGHTDATA_API_KEY"]);
    add("wolfram_alpha", "Wolfram Alpha MCP", "Computational knowledge, mathematics, formulas, physics", "📐", "npx", &["-y", "mcp-wolfram-alpha"], &["WOLFRAM_APP_ID"]);
    add("arxiv", "ArXiv Paper MCP", "Search academic research papers, preprints, and citations", "📄", "npx", &["-y", "mcp-arxiv"], &[]);
    add("wikipedia", "Wikipedia MCP", "Encyclopedia factual lookup, summaries, and history", "📚", "npx", &["-y", "@modelcontextprotocol/server-wikipedia"], &[]);
    add("puppeteer", "Puppeteer MCP", "Headless browser navigation, clicks, and page screenshots", "🎭", "npx", &["-y", "@modelcontextprotocol/server-puppeteer"], &[]);
    add("playwright", "Playwright MCP", "End-to-end browser automation and form interaction", "🎭", "npx", &["-y", "@executeautomation/playwright-mcp-server"], &[]);
    add("browserbase", "Browserbase MCP", "Cloud headless browser infrastructure with proxies", "☁️", "npx", &["-y", "@browserbase/mcp-server-browserbase"], &["BROWSERBASE_API_KEY"]);
    add("perplexity", "Perplexity MCP", "Sonar search models with cited web knowledge", "🧠", "npx", &["-y", "mcp-perplexity"], &["PERPLEXITY_API_KEY"]);
    add("serpapi", "SerpApi MCP", "Scrape search engine results pages from Google & Bing", "📡", "npx", &["-y", "mcp-serpapi"], &["SERPAPI_API_KEY"]);

    // Cloud & Infrastructure (9)
    add("aws", "AWS MCP", "Amazon Web Services S3, Lambda, EC2, CloudWatch", "☁️", "npx", &["-y", "@modelcontextprotocol/server-aws"], &["AWS_ACCESS_KEY_ID"]);
    add("cloudflare", "Cloudflare MCP", "Cloudflare Workers, DNS, KV, R2 bucket storage", "☁️", "npx", &["-y", "mcp-remote", "https://mcp.cloudflare.com/sse"], &["CLOUDFLARE_API_TOKEN"]);
    add("gcloud", "Google Cloud MCP", "Google Cloud Compute, Storage, IAM, Functions", "☁️", "npx", &["-y", "@google-cloud/mcp-server"], &["GOOGLE_CLOUD_CREDENTIALS"]);
    add("azure", "Microsoft Azure MCP", "Azure Virtual Machines, Blob Storage, Functions", "☁️", "npx", &["-y", "@azure/mcp"], &["AZURE_CREDENTIALS"]);
    add("vercel", "Vercel MCP", "Inspect deployments, edge domains, and env vars", "▲", "npx", &["-y", "@vercel/mcp"], &["VERCEL_TOKEN"]);
    add("terraform", "Terraform MCP", "Infrastructure as Code plan, apply, state reading", "🏗️", "npx", &["-y", "@hashicorp/terraform-mcp-server"], &[]);
    add("pulumi", "Pulumi MCP", "Modern infrastructure automation in TypeScript/Python", "🎈", "npx", &["-y", "@pulumi/mcp-server"], &[]);
    add("hetzner", "Hetzner Cloud MCP", "Manage Hetzner Cloud VMs, volumes, and firewalls", "🏢", "npx", &["-y", "mcp-hetzner"], &["HCLOUD_TOKEN"]);
    add("digitalocean", "DigitalOcean MCP", "Droplets, Kubernetes clusters, and Spaces storage", "🌊", "npx", &["-y", "mcp-digitalocean"], &["DO_TOKEN"]);

    // Productivity & Office (18)
    add("gdrive", "Google Drive MCP", "Google Drive files, folder search, downloads", "📁", "npx", &["-y", "@modelcontextprotocol/server-gdrive"], &["GOOGLE_DRIVE_TOKEN"]);
    add("gcalendar", "Google Calendar MCP", "Calendar events, scheduling, reminders", "📅", "npx", &["-y", "@modelcontextprotocol/server-gcalendar"], &["GOOGLE_CALENDAR_TOKEN"]);
    add("gsheets", "Google Sheets MCP", "Spreadsheet row reading, writing, and formulas", "📊", "npx", &["-y", "@modelcontextprotocol/server-gsheets"], &["GOOGLE_SHEETS_TOKEN"]);
    add("gdocs", "Google Docs MCP", "Read and write Google Docs documents directly", "📄", "npx", &["-y", "@modelcontextprotocol/server-gdocs"], &["GOOGLE_DOCS_TOKEN"]);
    add("notion", "Notion MCP", "Notion pages, databases, blocks, and comments", "📝", "npx", &["-y", "@modelcontextprotocol/server-notion"], &["NOTION_API_KEY"]);
    add("slack", "Slack MCP", "Workspace channels, direct messaging, bot notifications", "💬", "npx", &["-y", "@modelcontextprotocol/server-slack"], &["SLACK_BOT_TOKEN"]);
    add("discord", "Discord MCP", "Discord channels, roles, webhooks, community bots", "💬", "npx", &["-y", "mcp-remote", "https://mcp.discord.com/sse"], &["DISCORD_TOKEN"]);
    add("teams", "Microsoft Teams MCP", "Teams channels, chats, and meeting coordination", "👥", "npx", &["-y", "@modelcontextprotocol/server-teams"], &["TEAMS_TOKEN"]);
    add("m365", "Microsoft 365 MCP", "Outlook mail, OneDrive, and Microsoft Graph API", "📎", "npx", &["-y", "mcp-remote", "https://mcp.microsoft.com/sse"], &["M365_TOKEN"]);
    add("todoist", "Todoist MCP", "Task management, project lists, and due dates", "✅", "npx", &["-y", "@abhiz123/todoist-mcp-server"], &["TODOIST_API_TOKEN"]);
    add("asana", "Asana MCP", "Asana tasks, portfolios, and team milestones", "✅", "npx", &["-y", "mcp-remote", "https://mcp.asana.com/sse"], &["ASANA_TOKEN"]);
    add("trello", "Trello MCP", "Trello kanban boards, cards, and checklists", "🗂️", "npx", &["-y", "@modelcontextprotocol/server-trello"], &["TRELLO_API_KEY"]);
    add("clickup", "ClickUp MCP", "Manage ClickUp spaces, lists, custom fields, time tracking", "🎯", "npx", &["-y", "mcp-clickup"], &["CLICKUP_API_KEY"]);
    add("confluence", "Confluence MCP", "Atlassian Confluence team spaces, docs, and knowledge base", "📘", "npx", &["-y", "mcp-remote", "https://mcp.atlassian.com/confluence/sse"], &["CONFLUENCE_API_TOKEN"]);
    add("basecamp", "Basecamp MCP", "Project message boards, to-dos, schedules, and docs", "⛺", "npx", &["-y", "mcp-basecamp"], &["BASECAMP_ACCESS_TOKEN"]);
    add("airtable", "Airtable MCP", "Relational databases, bases, records, and formulas", "📑", "npx", &["-y", "mcp-airtable"], &["AIRTABLE_API_KEY"]);
    add("obsidian", "Obsidian Vault MCP", "Local Obsidian markdown notes, backlinks, and tags", "💎", "npx", &["-y", "mcp-obsidian"], &[]);
    add("readwise", "Readwise MCP", "Sync and query highlights, book notes, and articles", "📖", "npx", &["-y", "mcp-readwise"], &["READWISE_TOKEN"]);

    // Design & Creative (6)
    add("figma", "Figma MCP", "Design tokens, components, frames, and inspect data", "🎨", "npx", &["-y", "figma-developer-mcp"], &["FIGMA_ACCESS_TOKEN"]);
    add("canva", "Canva MCP", "Canva templates, graphics, and asset generation", "🎨", "npx", &["-y", "mcp-remote", "https://mcp.canva.com/sse"], &["CANVA_TOKEN"]);
    add("blender", "Blender MCP", "Blender 3D scene creation and Python render automation", "🎨", "npx", &["-y", "@modelcontextprotocol/server-blender"], &[]);
    add("unity", "Unity MCP", "Unity game engine scene graph, assets, and builds", "🎮", "npx", &["-y", "@modelcontextprotocol/server-unity"], &[]);
    add("unreal", "Unreal Engine MCP", "Unreal Engine 5 project assets and blueprint tools", "🎮", "npx", &["-y", "mcp-remote", "https://mcp.unrealengine.com/sse"], &[]);
    add("midjourney", "Midjourney MCP", "AI image generation, upscaling, and prompt styling", "🖼️", "npx", &["-y", "mcp-midjourney"], &["MIDJOURNEY_TOKEN"]);

    // AI / ML & Vector DBs (10)
    add("openai", "OpenAI MCP", "OpenAI models, DALL-E, embeddings, and Assistants", "🤖", "npx", &["-y", "@modelcontextprotocol/server-openai"], &["OPENAI_API_KEY"]);
    add("anthropic", "Anthropic Claude MCP", "Direct Claude 3.5 Sonnet tools, vision, and prompts", "🎭", "npx", &["-y", "@anthropic/mcp-claude"], &["ANTHROPIC_API_KEY"]);
    add("huggingface", "Hugging Face MCP", "Models, Spaces, datasets, and inference endpoints", "🤗", "npx", &["-y", "@modelcontextprotocol/server-huggingface"], &["HF_TOKEN"]);
    add("replicate", "Replicate MCP", "Run thousands of open-source models with cloud GPUs", "🔁", "npx", &["-y", "mcp-remote", "https://mcp.replicate.com/sse"], &["REPLICATE_API_TOKEN"]);
    add("langchain", "LangChain MCP", "LangChain tools, retrievers, and prompt templates", "🦜", "npx", &["-y", "mcp-remote", "https://mcp.langchain.com/sse"], &[]);
    add("pinecone", "Pinecone MCP", "High-performance vector database for semantic memory", "🌲", "npx", &["-y", "@pinecone/mcp-server"], &["PINECONE_API_KEY"]);
    add("qdrant", "Qdrant MCP", "Vector similarity search engine with payload filtering", "🔍", "npx", &["-y", "mcp-server-qdrant"], &["QDRANT_URL"]);
    add("weaviate", "Weaviate MCP", "Open-source AI-native vector database", "🔍", "npx", &["-y", "weaviate-mcp"], &["WEAVIATE_URL"]);
    add("chroma", "Chroma MCP", "Lightweight in-memory and embedded vector database", "🎨", "npx", &["-y", "chromadb-mcp"], &["CHROMA_URL"]);
    add("milvus", "Milvus MCP", "Cloud-native distributed vector database for massive scale", "⚡", "npx", &["-y", "mcp-milvus"], &["MILVUS_URI"]);

    // Business & Commerce (7)
    add("stripe", "Stripe MCP", "Payments, invoices, subscriptions, and balance ledger", "💳", "npx", &["-y", "@modelcontextprotocol/server-stripe"], &["STRIPE_SECRET_KEY"]);
    add("shopify", "Shopify MCP", "Product catalog, customer orders, inventory levels", "🛍️", "npx", &["-y", "@modelcontextprotocol/server-shopify"], &["SHOPIFY_TOKEN"]);
    add("salesforce", "Salesforce MCP", "CRM leads, accounts, contacts, and opportunities", "☁️", "npx", &["-y", "@modelcontextprotocol/server-salesforce"], &["SALESFORCE_TOKEN"]);
    add("hubspot", "HubSpot MCP", "Inbound marketing, CRM contacts, deals, and tickets", "🧲", "npx", &["-y", "@modelcontextprotocol/server-hubspot"], &["HUBSPOT_API_KEY"]);
    add("paypal", "PayPal MCP", "Transactions, payouts, disputes, and invoicing", "💳", "npx", &["-y", "mcp-remote", "https://mcp.paypal.com/sse"], &["PAYPAL_TOKEN"]);
    add("quickbooks", "QuickBooks MCP", "Accounting, invoices, expenses, and P&L reports", "📚", "npx", &["-y", "mcp-remote", "https://mcp.quickbooks.com/sse"], &["QUICKBOOKS_TOKEN"]);
    add("zendesk", "Zendesk MCP", "Customer support tickets, macros, and SLA metrics", "🎧", "npx", &["-y", "mcp-zendesk"], &["ZENDESK_TOKEN"]);

    // Finance & Web3 (5)
    add("yfinance", "Yahoo Finance MCP", "Real-time stock quotes, ETFs, financial statements, forex", "📈", "npx", &["-y", "mcp-yfinance"], &[]);
    add("alpha_vantage", "Alpha Vantage MCP", "Stock market data, technical indicators, economic data", "💹", "npx", &["-y", "mcp-alpha-vantage"], &["ALPHA_VANTAGE_API_KEY"]);
    add("coingecko", "CoinGecko Crypto MCP", "Crypto prices, token market cap, DEX liquidity, gas prices", "🦎", "npx", &["-y", "mcp-coingecko"], &["COINGECKO_API_KEY"]);
    add("etherscan", "Etherscan Blockchain MCP", "Ethereum smart contracts, transactions, wallet gas balances", "⛓️", "npx", &["-y", "mcp-etherscan"], &["ETHERSCAN_API_KEY"]);
    add("alchemy", "Alchemy Web3 RPC MCP", "Multi-chain RPC queries, NFT metadata, smart contract calls", "🔮", "npx", &["-y", "mcp-alchemy"], &["ALCHEMY_API_KEY"]);

    // Social & Messaging (7)
    add("telegram", "Telegram Bot MCP", "Send alerts, manage channels, polls, and incoming chats", "✈️", "npx", &["-y", "mcp-telegram"], &["TELEGRAM_BOT_TOKEN"]);
    add("whatsapp", "WhatsApp Business MCP", "Send automated WhatsApp notifications and customer chats", "💬", "npx", &["-y", "mcp-whatsapp"], &["WHATSAPP_API_TOKEN"]);
    add("twitter", "X / Twitter MCP", "Search tweets, analyze sentiment, post updates and threads", "🐦", "npx", &["-y", "mcp-twitter"], &["TWITTER_BEARER_TOKEN"]);
    add("reddit", "Reddit MCP", "Search subreddits, trending discussions, top submissions", "🤖", "npx", &["-y", "mcp-reddit"], &["REDDIT_CLIENT_ID", "REDDIT_CLIENT_SECRET"]);
    add("resend", "Resend Email MCP", "Send modern transactional emails and track delivery", "✉️", "npx", &["-y", "mcp-resend"], &["RESEND_API_KEY"]);
    add("sendgrid", "SendGrid Email MCP", "Send marketing emails, newsletter campaigns, templates", "📬", "npx", &["-y", "mcp-sendgrid"], &["SENDGRID_API_KEY"]);
    add("twilio", "Twilio SMS & Voice MCP", "Send SMS alerts, phone verification, and voice calls", "📞", "npx", &["-y", "mcp-twilio"], &["TWILIO_ACCOUNT_SID", "TWILIO_AUTH_TOKEN"]);

    // Security & Observability (10)
    add("datadog", "Datadog MCP", "Real-time metrics, traces, APM, and dashboard graphs", "🐶", "npx", &["-y", "mcp-remote", "https://mcp.datadoghq.com/sse"], &["DATADOG_API_KEY"]);
    add("grafana", "Grafana MCP", "Grafana dashboards, Prometheus metrics, Loki logs", "📈", "npx", &["-y", "@modelcontextprotocol/server-grafana"], &["GRAFANA_API_KEY"]);
    add("semgrep", "Semgrep MCP", "Static code analysis, SAST rules, security flaws", "🔒", "npx", &["-y", "semgrep-mcp"], &["SEMGREP_APP_TOKEN"]);
    add("snyk", "Snyk Security MCP", "Open-source vulnerability scanning and license checks", "🛡️", "npx", &["-y", "mcp-snyk"], &["SNYK_TOKEN"]);
    add("vault", "HashiCorp Vault MCP", "Hardware secrets management, key rotation, tokens", "🗝️", "npx", &["-y", "@hashicorp/vault-mcp"], &["VAULT_ADDR"]);
    add("onepassword", "1Password MCP", "Enterprise password vault, item retrieval, token security", "🔐", "npx", &["-y", "@1password/mcp-server"], &["OP_SERVICE_ACCOUNT_TOKEN"]);
    add("bitwarden", "Bitwarden MCP", "Open-source password vault and credential lookup", "🛡️", "npx", &["-y", "mcp-bitwarden"], &["BW_ACCESS_TOKEN"]);
    add("tailscale", "Tailscale VPN MCP", "Inspect Tailscale mesh nodes, device routes, connectivity", "🔗", "npx", &["-y", "mcp-tailscale"], &["TAILSCALE_API_KEY"]);
    add("shodan", "Shodan Network Recon MCP", "Search internet-facing devices, open ports, SSL certs", "📡", "npx", &["-y", "mcp-shodan"], &["SHODAN_API_KEY"]);
    add("virustotal", "VirusTotal Threat Intel MCP", "Inspect file hashes, IP reputation, domain malware scans", "🦠", "npx", &["-y", "mcp-virustotal"], &["VIRUSTOTAL_API_KEY"]);

    // Smart Home & Hardware IoT (2)
    add("home_assistant", "Home Assistant MCP", "Control smart lights, sensors, switches, climate", "🏠", "npx", &["-y", "@home-assistant/mcp-server"], &["HOME_ASSISTANT_TOKEN", "HOME_ASSISTANT_URL"]);
    add("mqtt", "MQTT IoT Broker MCP", "Publish and subscribe to IoT sensor message queues", "📻", "npx", &["-y", "mcp-mqtt"], &["MQTT_BROKER_URL"]);

    // Local Computer & System (4)
    add("shell", "Shell MCP", "Sovereign local bash/zsh command execution", "💻", "npx", &["-y", "@modelcontextprotocol/server-shell"], &[]);
    add("ssh", "SSH MCP", "Secure Shell remote terminal access and execution", "🔐", "npx", &["-y", "mcp-remote", "https://mcp.ssh.com/sse"], &["SSH_HOST"]);
    add("clipboard", "Clipboard MCP", "System clipboard reading, writing, history inspection", "📋", "npx", &["-y", "mcp-clipboard"], &[]);
    add("audio", "Audio MCP", "System audio playback, recording, and text-to-speech", "🎙️", "npx", &["-y", "mcp-audio"], &[]);

    v
}
