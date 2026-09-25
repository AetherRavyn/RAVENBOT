use serde::{Deserialize, Serialize};

/// Serde default for `verified` (existing persisted configs are treated as
/// verified; the built-in catalog overrides this per entry).
fn default_true() -> bool {
    true
}

/// Built-in connectors whose upstream launcher package could not be found on
/// npm/PyPI during the catalog audit. They are kept visible (for discovery and
/// future fixes) but flagged `verified = false` so the UI shows an honest
/// "unverified" badge instead of implying they work out of the box.
pub const UNVERIFIED_CONNECTORS: &[&str] = &[
    "github_actions", "railway", "flyio", "npm", "pypi", "planetscale",
    "surrealdb", "wolfram_alpha", "terraform", "vault", "snyk", "onepassword",
    "virustotal", "openai", "anthropic", "vercel", "alchemy",
];

/// Whether a built-in connector's launcher package is known to exist.
/// Custom/unknown servers are considered verified (the user configured them).
pub fn is_verified(id: &str) -> bool {
    !UNVERIFIED_CONNECTORS.contains(&id)
}

/// A single BYOC credential input the user fills for a connector
/// (e.g. Gmail: Google Project ID, Client ID, API key)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvField {
    pub key: String,
    pub label: String,
    pub description: String,
    pub required: bool,
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub placeholder: String,
}

impl EnvField {
    pub fn required(key: &str, label: &str, description: &str) -> Self {
        Self { key: key.to_string(), label: label.to_string(), description: description.to_string(), required: true, secret: true, placeholder: String::new() }
    }
    pub fn optional(key: &str, label: &str, description: &str) -> Self {
        Self { key: key.to_string(), label: label.to_string(), description: description.to_string(), required: false, secret: true, placeholder: String::new() }
    }
    pub fn non_secret(key: &str, label: &str, description: &str) -> Self {
        Self { key: key.to_string(), label: label.to_string(), description: description.to_string(), required: true, secret: false, placeholder: String::new() }
    }
}

/// How the MCP server is reached (mirrors cursor/plugins mcp.json)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    /// stdio: spawn `command args` and speak JSON-RPC
    Stdio,
    /// http: POST JSON-RPC to a URL (Cursor `"type": "http"`)
    Http,
}

impl Default for McpTransport {
    fn default() -> Self { McpTransport::Stdio }
}

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
    /// Rich BYOC descriptors (labels, required flags). Falls back to env_keys when empty.
    #[serde(default)]
    pub env_fields: Vec<EnvField>,
    /// Explicit URL for Cursor-style HTTP servers ( `"url": "https://..."` )
    #[serde(default)]
    pub url: Option<String>,
    /// Custom headers for HTTP servers (`{"Authorization": "Bearer ${KEY}"}`)
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    /// Cursor-style OAuth hint (`"auth": {"CLIENT_ID": ...}`)
    #[serde(default)]
    pub auth: Option<serde_json::Value>,
    /// Resolved transport. Stdio unless `mcp_type == "http"` or url is set.
    #[serde(default)]
    pub transport: McpTransport,
    /// Raw Cursor transport type (`"http"` / `"stdio"`), kept for import round-trip.
    #[serde(default)]
    pub mcp_type: Option<String>,
    pub enabled_by_default: bool,
    #[serde(default)]
    pub is_custom: bool,
    /// Whether the launcher package is known to exist (catalog audit).
    #[serde(default = "default_true")]
    pub verified: bool,
    /// Cursor marketplace origin (`cursor:<name>`, `cursor-third-party:<id>`, …)
    #[serde(default)]
    pub source: Option<String>,
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
    #[serde(default)]
    pub env_fields: Vec<EnvField>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub transport: McpTransport,
    pub enabled: bool,
    pub is_custom: bool,
    /// Whether the launcher package is known to exist (catalog audit).
    pub verified: bool,
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
    let mut add = |id: &str, name: &str, desc: &str, icon: &str, cmd: &str, args: &[&str], env: &[EnvField]| {
        v.push(McpServerConfig {
            id: id.to_string(),
            name: name.to_string(),
            description: desc.to_string(),
            category: category_of(id).to_string(),
            icon: icon.to_string(),
            command: cmd.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            env_keys: env.iter().map(|f| f.key.clone()).collect(),
            env_fields: env.to_vec(),
            url: None,
            headers: Default::default(),
            auth: None,
            transport: McpTransport::Stdio,
            mcp_type: None,
            enabled_by_default: matches!(id, "filesystem" | "git" | "puppeteer"),
            is_custom: false,
            verified: is_verified(id),
            source: None,
        });
    };
    // Shorthand for a single secret env var (project ID / API key / token)
    let req = |key: &str, label: &str, desc: &str| EnvField::required(key, label, desc);

    // Development & Coding (21)
    add("github", "GitHub MCP", "Repositories, issues, PRs, commits, code search", "🐙", "npx", &["-y", "@modelcontextprotocol/server-github"], &[req("GITHUB_PERSONAL_ACCESS_TOKEN","GitHub Personal Access Token","Create at github.com → Settings → Developer settings → Personal access tokens (classic). Needs repo scope.")]);
    add("gitlab", "GitLab MCP", "Projects, issues, merge requests, CI pipelines", "🦊", "npx", &["-y", "@modelcontextprotocol/server-gitlab"], &[req("GITLAB_PERSONAL_ACCESS_TOKEN","GitLab Personal Access Token","Create at gitlab.com → Preferences → Access Tokens. Needs api scope.")]);
    add("bitbucket", "Bitbucket MCP", "Repositories, PRs, workspaces, branch management", "🪣", "npx", &["-y", "mcp-remote", "https://mcp.bitbucket.org/sse"], &[req("BITBUCKET_TOKEN","Bitbucket Token","Bitbucket → Personal settings → App passwords.")]);
    add("sentry", "Sentry MCP", "Real-time error alerts, issues, stack trace debugging", "🚨", "npx", &["-y", "@sentry/mcp-server"], &[req("SENTRY_AUTH_TOKEN","Sentry Auth Token","sentry.io → Settings → Auth Tokens.")]);
    add("linear", "Linear MCP", "Issues, projects, cycles, triage workflows", "📐", "npx", &["-y", "mcp-remote", "https://mcp.linear.app/sse"], &[req("LINEAR_API_KEY","Linear API Key","linear.app → Settings → API. Personal API key.")]);
    add("jira", "Jira MCP", "Atlassian Jira project management and sprint tracking", "🗂️", "npx", &["-y", "mcp-remote", "https://mcp.atlassian.com/v1/sse"], &[req("JIRA_API_TOKEN","Jira API Token","Atlassian → Account settings → Security → API tokens. Also needs your Atlassian email as JIRA_EMAIL.")]);
    add("azure_devops", "Azure DevOps MCP", "Azure Repos, Boards, Pipelines, Artifacts", "🔷", "npx", &["-y", "@azure-devops/mcp"], &[req("AZURE_DEVOPS_PAT","Azure DevOps PAT","Azure DevOps → User settings → Personal access tokens.")]);
    add("github_actions", "GitHub Actions MCP", "Trigger workflows, inspect CI logs, retry jobs", "⚡", "npx", &["-y", "@modelcontextprotocol/server-github-actions"], &[req("GITHUB_PERSONAL_ACCESS_TOKEN","GitHub Personal Access Token","Create at github.com → Settings → Developer settings → Personal access tokens (classic). Needs repo scope.")]);
    add("railway", "Railway MCP", "Full-stack cloud deployments, environments, variables", "🚂", "npx", &["-y", "mcp-railway"], &[req("RAILWAY_API_TOKEN","Railway API Token","railway.app → Account → Tokens.")]);
    add("render", "Render MCP", "Deploy web services, databases, static sites, cron jobs", "🚀", "npx", &["-y", "mcp-render"], &[req("RENDER_API_KEY","Render API Key","dashboard.render.com → Account Settings → API Keys.")]);
    add("flyio", "Fly.io MCP", "Global micro-VM containers, scale apps, regions", "🎈", "npx", &["-y", "mcp-flyio"], &[req("FLY_API_TOKEN","Fly.io API Token","Run: flyctl auth token")]);
    add("argocd", "ArgoCD GitOps MCP", "Kubernetes continuous delivery, sync apps, status", "🐙", "npx", &["-y", "mcp-argocd"], &[req("ARGOCD_AUTH_TOKEN","ArgoCD Auth Token","ArgoCD → User Info → Get token, or argocd account generate-token.")]);
    add("posthog", "PostHog MCP", "Product analytics, feature flags, session replays, funnels", "🦔", "npx", &["-y", "@posthog/mcp"], &[req("POSTHOG_API_KEY","PostHog API Key","PostHog → Project settings → Project API key.")]);
    add("sonarqube", "SonarQube MCP", "Code quality gates, SAST security analysis, debt metrics", "🔍", "npx", &["-y", "mcp-sonarqube"], &[req("SONAR_TOKEN","SonarQube Token","SonarQube → My Account → Security → Generate token.")]);
    add("filesystem", "Filesystem MCP", "Direct local file inspection, reading, writing", "📁", "npx", &["-y", "@modelcontextprotocol/server-filesystem", "/tmp"], &[]);
    add("git", "Git MCP", "Local Git branches, log, diff, commit, rebase", "🌿", "uvx", &["mcp-server-git"], &[]);
    add("docker", "Docker MCP", "Container management, image building, runtime exec", "🐳", "npx", &["-y", "mcp-server-docker"], &[]);
    add("kubernetes", "Kubernetes MCP", "Cluster workloads, pod inspection, Helm deployments", "☸️", "npx", &["-y", "mcp-remote", "https://mcp.kubernetes.io/sse"], &[req("KUBECONFIG","Kubeconfig Path","~/.kube/config — path to your cluster config file.")]);
    add("postman", "Postman MCP", "API collections, environments, and mock runs", "📮", "npx", &["-y", "@postman/postman-mcp-server"], &[req("POSTMAN_API_KEY","Postman API Key","postman.com → Settings → API Keys.")]);
    add("npm", "NPM Registry MCP", "Search packages, inspect versions, verify dependencies", "📦", "npx", &["-y", "@modelcontextprotocol/server-npm"], &[]);
    add("pypi", "PyPI Registry MCP", "Python package index search and metadata inspection", "🐍", "npx", &["-y", "mcp-pypi"], &[]);

    // Databases (20)
    add("postgres", "PostgreSQL MCP", "Read/write SQL queries, schema inspection, tables", "🐘", "npx", &["-y", "@modelcontextprotocol/server-postgres"], &[req("POSTGRES_CONNECTION_STRING","Postgres Connection String","e.g. postgres://user:pass@localhost:5432/mydb")]);
    add("mysql", "MySQL MCP", "MySQL and MariaDB queries, transactions, tables", "🐬", "npx", &["-y", "@benborla29/mcp-server-mysql"], &[req("MYSQL_DSN","MySQL DSN","e.g. mysql://user:pass@localhost:3306/mydb")]);
    add("sqlite", "SQLite MCP", "Local SQLite database file querying and schema", "🗃️", "npx", &["-y", "mcp-server-sqlite"], &[]);
    add("duckdb", "DuckDB MCP", "Ultra-fast in-process analytical SQL for Parquet, CSV, Arrow (uses the local duckdb CLI)", "🦆", "ravenbot-builtin", &["duckdb"], &[]);
    add("mongodb", "MongoDB MCP", "Document collections, aggregation pipeline, indexes", "🍃", "npx", &["-y", "mcp-remote", "https://mcp.mongodb.com/sse"], &[req("MONGODB_URI","MongoDB URI","e.g. mongodb://localhost:27017/mydb")]);
    add("redis", "Redis MCP", "In-memory key-value cache, pub/sub, queues", "🔴", "npx", &["-y", "@gongrzhe/server-redis-mcp"], &[req("REDIS_URL","Redis URL","e.g. redis://localhost:6379")]);
    add("supabase", "Supabase MCP", "Postgres, Auth, Storage, Edge Functions", "⚡", "npx", &["-y", "@supabase/mcp-server-supabase"], &[req("SUPABASE_ACCESS_TOKEN","Supabase Access Token","supabase.com → Account → Access Tokens. Also set SUPABASE_URL if the server asks.")]);
    add("firebase", "Firebase MCP", "Firestore, Realtime Database, Cloud Auth", "🔥", "npx", &["-y", "@gannonh/firebase-mcp"], &[req("FIREBASE_TOKEN","Firebase Token","Run: firebase login:ci")]);
    add("neon", "Neon MCP", "Neon serverless Postgres branching and queries", "🌙", "npx", &["-y", "@neondatabase/mcp-server-neon"], &[req("NEON_API_KEY","Neon API Key","console.neon.tech → API Keys.")]);
    add("clickhouse", "ClickHouse MCP", "Fast analytical database queries and time-series", "📊", "npx", &["-y", "clickhouse-mcp"], &[req("CLICKHOUSE_DSN","ClickHouse DSN","e.g. http://localhost:8123/default")]);
    add("snowflake", "Snowflake MCP", "Snowflake Data Cloud queries and data warehouses", "❄️", "npx", &["-y", "mcp-remote", "https://mcp.snowflake.com/sse"], &[req("SNOWFLAKE_URI","Snowflake URI","Account + warehouse + user credentials string.")]);
    add("bigquery", "BigQuery MCP", "Google BigQuery enterprise analytics and datasets", "📈", "npx", &["-y", "@ergut/mcp-bigquery-server"], &[req("BIGQUERY_CREDENTIALS","BigQuery Credentials JSON","Paste the service-account JSON from Google Cloud Console.")]);
    add("planetscale", "PlanetScale MCP", "Serverless MySQL branching and migrations", "🪐", "npx", &["-y", "mcp-planetscale"], &[req("PLANETSCALE_SERVICE_TOKEN","PlanetScale Token","app.planetscale.com → Settings → Service tokens.")]);
    add("couchdb", "CouchDB MCP", "Apache CouchDB JSON documents and views", "🛋️", "npx", &["-y", "couchdb-mcp"], &[req("COUCHDB_URL","CouchDB URL","e.g. http://admin:pass@localhost:5984")]);
    add("elasticsearch", "Elasticsearch MCP", "Distributed enterprise full-text search and aggregations", "🔍", "npx", &["-y", "@elastic/mcp-server-elasticsearch"], &[req("ELASTICSEARCH_URL","Elasticsearch URL","e.g. http://localhost:9200"), req("ELASTICSEARCH_API_KEY","Elasticsearch API Key","Elastic → Stack Management → API Keys.")]);
    add("opensearch", "OpenSearch MCP", "OpenSearch search clusters, observability, vectors", "🔎", "uvx", &["opensearch-mcp-server-py"], &[req("OPENSEARCH_URL","OpenSearch URL","e.g. http://localhost:9200")]);
    add("neo4j", "Neo4j Graph MCP", "Graph database Cypher queries and relationship traversal", "🕸️", "uvx", &["mcp-neo4j-cypher"], &[req("NEO4J_URI","Neo4j URI","e.g. bolt://localhost:7687"), req("NEO4J_PASSWORD","Neo4j Password","Password for the neo4j user.")]);
    add("surrealdb", "SurrealDB MCP", "Multi-model database queries for document, graph, relational", "⚡", "npx", &["-y", "mcp-surrealdb"], &[req("SURREAL_URL","SurrealDB URL","e.g. ws://localhost:8000/rpc")]);
    add("timescaledb", "TimescaleDB MCP", "Time-series hypertables and analytics on PostgreSQL", "⏱️", "uvx", &["timescaledb-mcp"], &[req("TIMESCALE_CONNECTION_STRING","Timescale Connection String","Postgres-style connection string for Timescale.")]);
    add("meilisearch", "Meilisearch MCP", "Lightning-fast typo-tolerant full text search", "🔍", "npx", &["-y", "meilisearch-mcp"], &[req("MEILISEARCH_KEY","Meilisearch Master Key","Set in your meilisearch config.")]);

    // Web & Research (16)
    add("fetch", "Fetch MCP", "Raw web page content retrieval and HTML parsing", "🌐", "uvx", &["mcp-server-fetch"], &[]);
    add("brave_search", "Brave Search MCP", "Privacy-first real-time global web search", "🦁", "npx", &["-y", "@modelcontextprotocol/server-brave-search"], &[req("BRAVE_API_KEY","Brave Search API Key","brave.com/search/api → Get API key. Free tier available.")]);
    add("google_search", "Google Search MCP", "Google Search API for real-time web results", "🔍", "npx", &["-y", "mcp-remote", "https://mcp.googleapis.com/sse"], &[req("GOOGLE_API_KEY","Google API Key","console.cloud.google.com → APIs & Services → Credentials → API key.")]);
    add("tavily", "Tavily MCP", "AI-optimized search engineered for LLM agents", "🔎", "npx", &["-y", "mcp-remote", "https://mcp.tavily.com/sse"], &[req("TAVILY_API_KEY","Tavily API Key","tavily.com → API Keys.")]);
    add("exa", "Exa MCP", "Neural semantic web search and link embeddings", "✨", "npx", &["-y", "exa-mcp-server"], &[req("EXA_API_KEY","Exa API Key","dashboard.exa.ai → API Keys.")]);
    add("firecrawl", "Firecrawl MCP", "Crawl full domains and convert pages to clean markdown", "🔥", "npx", &["-y", "firecrawl-mcp"], &[req("FIRECRAWL_API_KEY","Firecrawl API Key","firecrawl.dev → API Keys.")]);
    add("scrapfly", "Scrapfly Anti-Bot Scraper", "Bypass anti-bot protections, rotate residential IPs", "🪰", "npx", &["-y", "scrapfly-mcp"], &[req("SCRAPFLY_API_KEY","Scrapfly API Key","scrapfly.io → API Keys.")]);
    add("brightdata", "Bright Data Web Unlocker", "Automated CAPTCHA bypass, residential browser proxies", "🌐", "npx", &["-y", "@brightdata/mcp"], &[req("BRIGHTDATA_API_KEY","Bright Data API Key","brightdata.com → API keys.")]);
    add("wolfram_alpha", "Wolfram Alpha MCP", "Computational knowledge, mathematics, formulas, physics", "📐", "npx", &["-y", "mcp-wolfram-alpha"], &[req("WOLFRAM_APP_ID","Wolfram App ID","developer.wolframalpha.com → Get App ID.")]);
    add("arxiv", "ArXiv Paper MCP", "Search academic research papers, preprints, and citations", "📄", "uvx", &["arxiv-mcp-server"], &[]);
    add("wikipedia", "Wikipedia MCP", "Encyclopedia factual lookup, summaries, and history", "📚", "npx", &["-y", "wikipedia-mcp"], &[]);
    add("puppeteer", "Puppeteer MCP", "Headless browser navigation, clicks, and page screenshots", "🎭", "npx", &["-y", "@modelcontextprotocol/server-puppeteer"], &[]);
    add("playwright", "Playwright MCP", "End-to-end browser automation and form interaction", "🎭", "npx", &["-y", "@executeautomation/playwright-mcp-server"], &[]);
    add("browserbase", "Browserbase MCP", "Cloud headless browser infrastructure with proxies", "☁️", "npx", &["-y", "@browserbasehq/mcp"], &[req("BROWSERBASE_API_KEY","Browserbase Api Key","Paste your BROWSERBASE_API_KEY.")]);
    add("perplexity", "Perplexity MCP", "Sonar search models with cited web knowledge", "🧠", "npx", &["-y", "mcp-perplexity"], &[req("PERPLEXITY_API_KEY","Perplexity API Key","perplexity.ai → Settings → API.")]);
    add("serpapi", "SerpApi MCP", "Scrape search engine results pages from Google & Bing", "📡", "npx", &["-y", "mcp-serpapi"], &[req("SERPAPI_API_KEY","SerpAPI Key","serpapi.com → Dashboard → API Key.")]);

    // Cloud & Infrastructure (9)
    add("aws", "AWS MCP", "Amazon Web Services S3, Lambda, EC2, CloudWatch", "☁️", "uvx", &["awslabs.core-mcp-server"], &[req("AWS_ACCESS_KEY_ID","AWS Access Key ID","AWS Console → IAM → Users → Security credentials.")]);
    add("cloudflare", "Cloudflare MCP", "Cloudflare Workers, DNS, KV, R2 bucket storage", "☁️", "npx", &["-y", "mcp-remote", "https://mcp.cloudflare.com/sse"], &[req("CLOUDFLARE_API_TOKEN","Cloudflare API Token","dash.cloudflare.com → My Profile → API Tokens.")]);
    add("gcloud", "Google Cloud MCP", "Google Cloud Compute, Storage, IAM, Functions", "☁️", "npx", &["-y", "@google-cloud/gcloud-mcp"], &[req("GOOGLE_CLOUD_CREDENTIALS","Google Cloud Credentials JSON","Service-account JSON from Google Cloud Console.")]);
    add("azure", "Microsoft Azure MCP", "Azure Virtual Machines, Blob Storage, Functions", "☁️", "npx", &["-y", "@azure/mcp"], &[req("AZURE_CREDENTIALS","Azure Credentials JSON","Service principal JSON from Azure Portal.")]);
    add("vercel", "Vercel MCP", "Inspect deployments, edge domains, and env vars", "▲", "npx", &["-y", "@vercel/mcp"], &[req("VERCEL_TOKEN","Vercel Token","vercel.com → Settings → Tokens.")]);
    add("terraform", "Terraform MCP", "Infrastructure as Code plan, apply, state reading", "🏗️", "npx", &["-y", "@hashicorp/terraform-mcp-server"], &[]);
    add("pulumi", "Pulumi MCP", "Modern infrastructure automation in TypeScript/Python", "🎈", "npx", &["-y", "@pulumi/mcp-server"], &[]);
    add("hetzner", "Hetzner Cloud MCP", "Manage Hetzner Cloud VMs, volumes, and firewalls", "🏢", "npx", &["-y", "hcloud-mcp"], &[req("HCLOUD_TOKEN","Hetzner Cloud Token","console.hetzner.cloud → Security → API Tokens.")]);
    add("digitalocean", "DigitalOcean MCP", "Droplets, Kubernetes clusters, and Spaces storage", "🌊", "npx", &["-y", "mcp-digitalocean"], &[req("DO_TOKEN","DigitalOcean Token","cloud.digitalocean.com → API → Generate token.")]);

    // Productivity & Office (18)
    add("gdrive", "Google Drive MCP", "Google Drive files, folder search, downloads", "📁", "npx", &["-y", "@modelcontextprotocol/server-gdrive"], &[req("GOOGLE_DRIVE_TOKEN","Google OAuth Token","Your own Google OAuth access token, or Client ID + secret below.")]);
    add("gcalendar", "Google Calendar MCP", "Calendar events, scheduling, reminders", "📅", "npx", &["-y", "mcp-google-calendar"], &[req("GOOGLE_CALENDAR_TOKEN","Google OAuth Token","Your own Google OAuth access token for Calendar.")]);
    add("gsheets", "Google Sheets MCP", "Spreadsheet row reading, writing, and formulas", "📊", "npx", &["-y", "mcp-google-sheets"], &[req("GOOGLE_SHEETS_TOKEN","Google OAuth Token","Your own Google OAuth access token for Sheets.")]);
    add("gdocs", "Google Docs MCP", "Read and write Google Docs documents directly", "📄", "npx", &["-y", "mcp-google-docs"], &[req("GOOGLE_DOCS_TOKEN","Google OAuth Token","Your own Google OAuth access token for Docs.")]);
    add("notion", "Notion MCP", "Notion pages, databases, blocks, and comments", "📝", "npx", &["-y", "@notionhq/notion-mcp-server"], &[req("NOTION_TOKEN","Notion Integration Token","notion.so → Settings → Connections → Develop → New integration → copy Internal Integration Secret (starts with ntn_).")]);
    add("slack", "Slack MCP", "Workspace channels, direct messaging, bot notifications", "💬", "npx", &["-y", "@modelcontextprotocol/server-slack"], &[req("SLACK_BOT_TOKEN","Slack Bot Token","api.slack.com → Your App → OAuth & Permissions → Bot User OAuth Token (starts with xoxb-).")]);
    add("discord", "Discord MCP", "Discord channels, roles, webhooks, community bots", "💬", "npx", &["-y", "mcp-remote", "https://mcp.discord.com/sse"], &[req("DISCORD_TOKEN","Discord Bot Token","discord.com/developers → Application → Bot → Token.")]);
    add("teams", "Microsoft Teams MCP", "Teams channels, chats, and meeting coordination", "👥", "npx", &["-y", "teams-mcp"], &[req("TEAMS_TOKEN","Teams / Graph Token","Azure Portal → App registrations → your app → client secret, or a Graph access token.")]);
    add("m365", "Microsoft 365 MCP", "Outlook mail, OneDrive, and Microsoft Graph API", "📎", "npx", &["-y", "mcp-remote", "https://mcp.microsoft.com/sse"], &[req("M365_TOKEN","Microsoft 365 Token","Microsoft Graph access token for your tenant.")]);
    add("todoist", "Todoist MCP", "Task management, project lists, and due dates", "✅", "npx", &["-y", "@abhiz123/todoist-mcp-server"], &[req("TODOIST_API_TOKEN","Todoist API Token","todoist.com → Settings → Integrations → Developer → API token.")]);
    add("asana", "Asana MCP", "Asana tasks, portfolios, and team milestones", "✅", "npx", &["-y", "mcp-remote", "https://mcp.asana.com/sse"], &[req("ASANA_TOKEN","Asana PAT","app.asana.com → My Profile → Apps → Manage Developer Apps → PAT.")]);
    add("trello", "Trello MCP", "Trello kanban boards, cards, and checklists", "🗂️", "npx", &["-y", "@delorenj/mcp-server-trello"], &[req("TRELLO_API_KEY","Trello API Key","trello.com/app-key → Key. Also needs TRELLO_TOKEN.")]);
    add("clickup", "ClickUp MCP", "Manage ClickUp spaces, lists, custom fields, time tracking", "🎯", "npx", &["-y", "mcp-clickup"], &[req("CLICKUP_API_KEY","ClickUp API Key","app.clickup.com → Apps → API token.")]);
    add("confluence", "Confluence MCP", "Atlassian Confluence team spaces, docs, and knowledge base", "📘", "npx", &["-y", "mcp-remote", "https://mcp.atlassian.com/confluence/sse"], &[req("CONFLUENCE_API_TOKEN","Confluence API Token","Atlassian API token (same as Jira). Also needs CONFLUENCE_EMAIL and CONFLUENCE_BASE_URL.")]);
    add("basecamp", "Basecamp MCP", "Project message boards, to-dos, schedules, and docs", "⛺", "npx", &["-y", "mcp-basecamp"], &[req("BASECAMP_ACCESS_TOKEN","Basecamp Token","launchpad.37signals.com → Personal token.")]);
    add("airtable", "Airtable MCP", "Relational databases, bases, records, and formulas", "📑", "npx", &["-y", "mcp-airtable"], &[req("AIRTABLE_API_KEY","Airtable PAT","airtable.com/create/tokens → Personal access token (starts with pat).")]);
    add("obsidian", "Obsidian Vault MCP", "Local Obsidian markdown notes, backlinks, and tags", "💎", "npx", &["-y", "mcp-obsidian"], &[]);
    add("readwise", "Readwise MCP", "Sync and query highlights, book notes, and articles", "📖", "npx", &["-y", "readwise-mcp"], &[req("READWISE_TOKEN","Readwise Token","readwise.io → Access Token page.")]);

    // Design & Creative (6)
    add("figma", "Figma MCP", "Design tokens, components, frames, and inspect data", "🎨", "npx", &["-y", "figma-developer-mcp"], &[req("FIGMA_ACCESS_TOKEN","Figma Access Token","figma.com → Settings → Personal access tokens (starts with figd_).")]);
    add("canva", "Canva MCP", "Canva templates, graphics, and asset generation", "🎨", "npx", &["-y", "mcp-remote", "https://mcp.canva.com/sse"], &[req("CANVA_TOKEN","Canva Token","Canva Connect API token for your team.")]);
    add("blender", "Blender MCP", "Blender 3D scene creation and Python render automation", "🎨", "npx", &["-y", "blender-mcp"], &[]);
    add("unity", "Unity MCP", "Unity game engine scene graph, assets, and builds", "🎮", "npx", &["-y", "unity-mcp"], &[]);
    add("unreal", "Unreal Engine MCP", "Unreal Engine 5 project assets and blueprint tools", "🎮", "npx", &["-y", "mcp-remote", "https://mcp.unrealengine.com/sse"], &[]);
    add("midjourney", "Midjourney MCP", "AI image generation, upscaling, and prompt styling", "🖼️", "npx", &["-y", "mcp-midjourney"], &[req("MIDJOURNEY_TOKEN","Midjourney Token","Your Midjourney API token if using a proxy service.")]);

    // AI / ML & Vector DBs (10)
    add("openai", "OpenAI MCP", "OpenAI models, DALL-E, embeddings, and Assistants", "🤖", "npx", &["-y", "@modelcontextprotocol/server-openai"], &[req("OPENAI_API_KEY","OpenAI API Key","platform.openai.com → API keys (starts with sk-).")]);
    add("anthropic", "Anthropic Claude MCP", "Direct Claude 3.5 Sonnet tools, vision, and prompts", "🎭", "npx", &["-y", "@anthropic/mcp-claude"], &[req("ANTHROPIC_API_KEY","Anthropic API Key","console.anthropic.com → API Keys (starts with sk-ant-).")]);
    add("huggingface", "Hugging Face MCP", "Models, Spaces, datasets, and inference endpoints", "🤗", "npx", &["-y", "huggingface-mcp"], &[req("HF_TOKEN","Hugging Face Token","huggingface.co → Settings → Access Tokens (starts with hf_).")]);
    add("replicate", "Replicate MCP", "Run thousands of open-source models with cloud GPUs", "🔁", "npx", &["-y", "mcp-remote", "https://mcp.replicate.com/sse"], &[req("REPLICATE_API_TOKEN","Replicate API Token","replicate.com → Account → API tokens (starts with r8_).")]);
    add("langchain", "LangChain MCP", "LangChain tools, retrievers, and prompt templates", "🦜", "npx", &["-y", "mcp-remote", "https://mcp.langchain.com/sse"], &[]);
    add("pinecone", "Pinecone MCP", "High-performance vector database for semantic memory", "🌲", "npx", &["-y", "@pinecone-database/mcp"], &[req("PINECONE_API_KEY","Pinecone API Key","app.pinecone.io → API Keys.")]);
    add("qdrant", "Qdrant MCP", "Vector similarity search engine with payload filtering", "🔍", "npx", &["-y", "mcp-server-qdrant"], &[req("QDRANT_URL","Qdrant URL","e.g. http://localhost:6333")]);
    add("weaviate", "Weaviate MCP", "Open-source AI-native vector database", "🔍", "npx", &["-y", "weaviate-mcp"], &[req("WEAVIATE_URL","Weaviate URL","e.g. http://localhost:8080")]);
    add("chroma", "Chroma MCP", "Lightweight in-memory and embedded vector database", "🎨", "npx", &["-y", "chromadb-mcp"], &[req("CHROMA_URL","Chroma URL","e.g. http://localhost:8000")]);
    add("milvus", "Milvus MCP", "Cloud-native distributed vector database for massive scale", "⚡", "npx", &["-y", "mcp-milvus"], &[req("MILVUS_URI","Milvus URI","e.g. http://localhost:19530")]);

    // Business & Commerce (7)
    add("stripe", "Stripe MCP", "Payments, invoices, subscriptions, and balance ledger", "💳", "npx", &["-y", "@stripe/mcp", "--tools=all"], &[req("STRIPE_SECRET_KEY","Stripe Secret Key","dashboard.stripe.com → Developers → API keys (starts with sk_live_ or sk_test_).")]);
    add("shopify", "Shopify MCP", "Product catalog, customer orders, inventory levels", "🛍️", "npx", &["-y", "@shopify/dev-mcp"], &[req("SHOPIFY_TOKEN","Shopify Access Token","Shopify admin → Apps → Develop apps → Admin API access token (starts with shpat_). Also needs SHOPIFY_STORE.")]);
    add("salesforce", "Salesforce MCP", "CRM leads, accounts, contacts, and opportunities", "☁️", "npx", &["-y", "@salesforce/mcp"], &[req("SALESFORCE_TOKEN","Salesforce Token","Setup → Apps → Connected Apps → OAuth token, or username + password + security token.")]);
    add("hubspot", "HubSpot MCP", "Inbound marketing, CRM contacts, deals, and tickets", "🧲", "npx", &["-y", "@hubspot/mcp-server"], &[req("HUBSPOT_API_KEY","HubSpot Private App Token","HubSpot → Settings → Integrations → Private Apps (starts with pat-).")]);
    add("paypal", "PayPal MCP", "Transactions, payouts, disputes, and invoicing", "💳", "npx", &["-y", "mcp-remote", "https://mcp.paypal.com/sse"], &[req("PAYPAL_TOKEN","PayPal Access Token","developer.paypal.com → Apps & Credentials → Client ID + secret exchanged for token.")]);
    add("quickbooks", "QuickBooks MCP", "Accounting, invoices, expenses, and P&L reports", "📚", "npx", &["-y", "mcp-remote", "https://mcp.quickbooks.com/sse"], &[req("QUICKBOOKS_TOKEN","QuickBooks Token","developer.intuit.com → OAuth access token for your company.")]);
    add("zendesk", "Zendesk MCP", "Customer support tickets, macros, and SLA metrics", "🎧", "npx", &["-y", "zendesk-mcp"], &[req("ZENDESK_TOKEN","Zendesk API Token","Zendesk Admin → API → Token access. Also needs ZENDESK_SUBDOMAIN.")]);

    // Finance & Web3 (5)
    add("yfinance", "Yahoo Finance MCP", "Real-time stock quotes, ETFs, financial statements, forex", "📈", "npx", &["-y", "yfinance-mcp"], &[]);
    add("alpha_vantage", "Alpha Vantage MCP", "Stock market data, technical indicators, economic data", "💹", "npx", &["-y", "alpha-vantage-mcp"], &[req("ALPHA_VANTAGE_API_KEY","Alpha Vantage Key","alphavantage.co → Get free API key.")]);
    add("coingecko", "CoinGecko Crypto MCP", "Crypto prices, token market cap, DEX liquidity, gas prices", "🦎", "npx", &["-y", "coingecko-mcp"], &[req("COINGECKO_API_KEY","CoinGecko API Key","coingecko.com → API → Demo key is free.")]);
    add("etherscan", "Etherscan Blockchain MCP", "Ethereum smart contracts, transactions, wallet gas balances", "⛓️", "npx", &["-y", "etherscan-mcp"], &[req("ETHERSCAN_API_KEY","Etherscan API Key","etherscan.io → APIs → Create API key.")]);
    add("alchemy", "Alchemy Web3 RPC MCP", "Multi-chain RPC queries, NFT metadata, smart contract calls", "🔮", "npx", &["-y", "mcp-alchemy"], &[req("ALCHEMY_API_KEY","Alchemy API Key","dashboard.alchemy.com → Apps → API Key.")]);

    // Social & Messaging (7)
    add("telegram", "Telegram Bot MCP", "Send alerts, manage channels, polls, and incoming chats", "✈️", "npx", &["-y", "mcp-telegram"], &[req("TELEGRAM_BOT_TOKEN","Telegram Bot Token","Message @BotFather on Telegram → /newbot → copy token.")]);
    add("whatsapp", "WhatsApp Business MCP", "Send automated WhatsApp notifications and customer chats", "💬", "npx", &["-y", "mcp-whatsapp"], &[req("WHATSAPP_API_TOKEN","WhatsApp API Token","Meta Developers → WhatsApp → API token for your business number.")]);
    add("twitter", "X / Twitter MCP", "Search tweets, analyze sentiment, post updates and threads", "🐦", "npx", &["-y", "@enescinar/twitter-mcp"], &[req("TWITTER_BEARER_TOKEN","X Bearer Token","developer.x.com → Project → Keys and tokens → Bearer Token.")]);
    add("reddit", "Reddit MCP", "Search subreddits, trending discussions, top submissions", "🤖", "npx", &["-y", "mcp-reddit"], &[req("REDDIT_CLIENT_ID","Reddit Client ID","reddit.com/prefs/apps → Create app → client ID."), req("REDDIT_CLIENT_SECRET","Reddit Client Secret","Pair with the Client ID above.")]);
    add("resend", "Resend Email MCP", "Send modern transactional emails and track delivery", "✉️", "npx", &["-y", "mcp-resend"], &[req("RESEND_API_KEY","Resend API Key","resend.com → API Keys (starts with re_).")]);
    add("sendgrid", "SendGrid Email MCP", "Send marketing emails, newsletter campaigns, templates", "📬", "npx", &["-y", "sendgrid-mcp"], &[req("SENDGRID_API_KEY","SendGrid API Key","app.sendgrid.com → Settings → API Keys (starts with SG.).")]);
    add("twilio", "Twilio SMS & Voice MCP", "Send SMS alerts, phone verification, and voice calls", "📞", "npx", &["-y", "twilio-mcp"], &[req("TWILIO_ACCOUNT_SID","Twilio Account SID","console.twilio.com → Account Info → Account SID (starts with AC)."), req("TWILIO_AUTH_TOKEN","Twilio Auth Token","Pair with the Account SID above.")]);

    // Security & Observability (10)
    add("datadog", "Datadog MCP", "Real-time metrics, traces, APM, and dashboard graphs", "🐶", "npx", &["-y", "mcp-remote", "https://mcp.datadoghq.com/sse"], &[req("DATADOG_API_KEY","Datadog API Key","app.datadoghq.com → Organization Settings → API Keys.")]);
    add("grafana", "Grafana MCP", "Grafana dashboards, Prometheus metrics, Loki logs", "📈", "uvx", &["mcp-grafana"], &[req("GRAFANA_API_KEY","Grafana API Key","Grafana → Administration → API keys.")]);
    add("semgrep", "Semgrep MCP", "Static code analysis, SAST rules, security flaws", "🔒", "uvx", &["semgrep-mcp"], &[req("SEMGREP_APP_TOKEN","Semgrep Token","semgrep.dev → Settings → Tokens.")]);
    add("snyk", "Snyk Security MCP", "Open-source vulnerability scanning and license checks", "🛡️", "npx", &["-y", "mcp-snyk"], &[req("SNYK_TOKEN","Snyk Token","app.snyk.io → Account Settings → API Token.")]);
    add("vault", "HashiCorp Vault MCP", "Hardware secrets management, key rotation, tokens", "🗝️", "npx", &["-y", "@hashicorp/vault-mcp"], &[req("VAULT_ADDR","Vault Address","e.g. http://127.0.0.1:8200")]);
    add("onepassword", "1Password MCP", "Enterprise password vault, item retrieval, token security", "🔐", "npx", &["-y", "@1password/mcp-server"], &[req("OP_SERVICE_ACCOUNT_TOKEN","Op Service Account Token","Paste your OP_SERVICE_ACCOUNT_TOKEN.")]);
    add("bitwarden", "Bitwarden MCP", "Open-source password vault and credential lookup", "🛡️", "npx", &["-y", "@bitwarden/mcp-server"], &[req("BW_ACCESS_TOKEN","Bw Access Token","Paste your BW_ACCESS_TOKEN.")]);
    add("tailscale", "Tailscale VPN MCP", "Inspect Tailscale mesh nodes, device routes, connectivity", "🔗", "npx", &["-y", "mcp-tailscale"], &[req("TAILSCALE_API_KEY","Tailscale API Key","login.tailscale.com/admin/settings/keys → Generate API key (starts with tskey-api-).")]);
    add("shodan", "Shodan Network Recon MCP", "Search internet-facing devices, open ports, SSL certs", "📡", "npx", &["-y", "shodan-mcp"], &[req("SHODAN_API_KEY","Shodan API Key","account.shodan.io → API Key.")]);
    add("virustotal", "VirusTotal Threat Intel MCP", "Inspect file hashes, IP reputation, domain malware scans", "🦠", "npx", &["-y", "mcp-virustotal"], &[req("VIRUSTOTAL_API_KEY","VirusTotal API Key","virustotal.com → API key page.")]);

    // Smart Home & Hardware IoT (2)
    add("home_assistant", "Home Assistant MCP", "Control smart lights, sensors, switches, climate", "🏠", "uvx", &["homeassistant-mcp"], &[req("HOME_ASSISTANT_TOKEN","Home Assistant Token","Paste your HOME_ASSISTANT_TOKEN."), req("HOME_ASSISTANT_URL","Home Assistant Url","Paste your HOME_ASSISTANT_URL.")]);
    add("mqtt", "MQTT IoT Broker MCP", "Publish and subscribe to IoT sensor message queues", "📻", "npx", &["-y", "mqtt-mcp"], &[req("MQTT_BROKER_URL","Mqtt Broker Url","Paste your MQTT_BROKER_URL.")]);

    // Local Computer & System (4)
    add("shell", "Shell MCP", "Sovereign local bash/zsh command execution", "💻", "npx", &["-y", "shell-mcp-server"], &[]);
    add("ssh", "SSH MCP", "Secure Shell remote terminal access and execution", "🔐", "npx", &["-y", "mcp-remote", "https://mcp.ssh.com/sse"], &[req("SSH_HOST","Ssh Host","Paste your SSH_HOST.")]);
    add("clipboard", "Clipboard MCP", "System clipboard reading, writing, history inspection", "📋", "npx", &["-y", "mcp-clipboard"], &[]);
    add("audio", "Audio MCP", "System audio playback, recording, and text-to-speech", "🎙️", "npx", &["-y", "mcp-audio"], &[]);

    v
}

#[cfg(test)]
mod catalog_tests {
    use super::*;

    fn find(id: &str) -> McpServerConfig {
        all_servers()
            .into_iter()
            .find(|s| s.id == id)
            .unwrap_or_else(|| panic!("connector '{id}' missing from catalog"))
    }

    #[test]
    fn known_connectors_are_marked_verified() {
        for id in ["filesystem", "git", "github", "postgres", "slack", "notion"] {
            assert!(find(id).verified, "{id} should be verified");
        }
    }

    #[test]
    fn unresolvable_connectors_are_marked_unverified() {
        for id in UNVERIFIED_CONNECTORS {
            assert!(!find(id).verified, "{id} should be unverified");
        }
        assert!(!is_verified("terraform"));
        assert!(is_verified("custom_user_server"));
    }

    #[test]
    fn corrected_connectors_point_at_real_packages() {
        // These were previously pointed at packages that do not exist.
        assert_eq!(find("mysql").args, vec!["-y", "@benborla29/mcp-server-mysql"]);
        assert_eq!(find("postman").args, vec!["-y", "@postman/postman-mcp-server"]);
        assert_eq!(find("meilisearch").args, vec!["-y", "meilisearch-mcp"]);
        assert_eq!(find("neo4j").command, "uvx");
        assert_eq!(find("neo4j").args, vec!["mcp-neo4j-cypher"]);
    }
}
