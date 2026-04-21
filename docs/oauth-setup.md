# Google OAuth Setup

The Parish web server supports optional Google sign-in so visitors can claim their anonymous session across devices. This doc walks through obtaining credentials from Google Cloud Console and wiring them into the server.

## What the server expects

The web server (`crates/parish-server/`) reads three environment variables on startup:

| Variable | Purpose |
|----------|---------|
| `GOOGLE_CLIENT_ID` | OAuth client ID from Google Cloud Console |
| `GOOGLE_CLIENT_SECRET` | OAuth client secret from Google Cloud Console |
| `PARISH_PUBLIC_URL` | Public base URL of the server (defaults to `http://localhost:3001`). Falls back to `PARISH_BASE_URL` if unset. |

These are loaded via `dotenvy::dotenv()`, so a `.env` file at the repo root works. If either `GOOGLE_CLIENT_ID` or `GOOGLE_CLIENT_SECRET` is missing or empty, OAuth is silently disabled and the `/auth/login/google` + `/auth/callback/google` routes are not registered.

Relevant code:
- `crates/parish-server/src/lib.rs` — `build_oauth_config()` reads the env vars
- `crates/parish-server/src/auth.rs` — OAuth flow, hard-codes the `openid email profile` scopes and the `/auth/callback/google` redirect path

## Google Cloud Console walkthrough

1. **Create or select a project** at https://console.cloud.google.com/. Click the project dropdown at the top → "New Project" → name it (e.g. "Parish") → Create.

2. **Configure the OAuth consent screen:**
   - Navigation menu → "APIs & Services" → "OAuth consent screen"
   - User type: **External** (unless you have a Google Workspace org and want internal-only)
   - Fill in app name, user support email, and developer contact email
   - Add scopes: `openid`, `email`, `profile`
   - Add your own Google account as a **test user** — while the app is in "Testing" status, only listed test users can log in

3. **Create OAuth credentials:**
   - "APIs & Services" → "Credentials" → "Create Credentials" → **"OAuth client ID"**
   - Application type: **Web application**
   - Name: anything (e.g. "Parish Web Server")
   - **Authorized redirect URIs** — add one per environment you'll run in:
     - Local dev: `http://localhost:3001/auth/callback/google`
     - Production: `https://your-domain.example.com/auth/callback/google`
   - Click Create

4. **Copy the Client ID and Client Secret** from the dialog that appears.

## Local testing

Put the credentials into a `.env` file at the repo root:

```env
GOOGLE_CLIENT_ID=your-client-id.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=your-client-secret
PARISH_PUBLIC_URL=http://localhost:3001
```

Start the web server. On startup you should see:

```
INFO Google OAuth enabled
```

in the logs — that confirms both credentials were picked up. Then visit http://localhost:3001, trigger login, and verify you're redirected back to `/` after consenting.

## Gotchas

- **Test users only.** While the consent screen is in "Testing" status, Google rejects logins from accounts not on the test user list. Publish the app (or add more test users) to widen access.
- **Redirect URI must match exactly.** The code builds the callback URL as `${PARISH_PUBLIC_URL}/auth/callback/google` (with any trailing slash trimmed via `trim_end_matches('/')`). Whatever you register in Google Cloud must match byte-for-byte, including scheme (`http` vs `https`) and port. If `PARISH_PUBLIC_URL` is not set, `PARISH_BASE_URL` is used as a fallback.
- **Silent disable.** Missing or empty credentials don't raise an error — the auth routes just aren't registered. If `/auth/login/google` returns 404, check that both env vars are actually set in the process's environment.

## Railway deployment

When deploying to Railway (or any other host):

1. Set `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, and `PARISH_PUBLIC_URL` in the service's environment variables. `PARISH_PUBLIC_URL` must be the public URL Railway assigns you (e.g. `https://parish-production.up.railway.app`).
2. Add the production callback URL (`${PARISH_PUBLIC_URL}/auth/callback/google`) to the authorized redirect URIs list in Google Cloud Console **before** the first production login attempt — Google rejects unregistered redirect URIs.
3. You can reuse the same OAuth client for local and production by listing both redirect URIs on the same credential, or create separate clients per environment.
