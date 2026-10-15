# OAuth for MCP clients

ArrPi can accept OAuth access tokens on `/v1/mcp` alongside its existing account
tokens. Your external authorization server handles sign-in, consent, client
registration and token refresh. ArrPi verifies access tokens and maps an explicitly
approved identity to an existing ArrPi requester account. It does not issue tokens
or provide a login server.

## Configure the authorization server

Use an OAuth authorization server that supports authorization-code clients with
PKCE S256 and can issue signed JWT access tokens for ArrPi's public MCP URL. For an
endpoint of `https://arrpi.example.com/v1/mcp`, configure:

| Setting                            | Value                                                             |
| ---------------------------------- | ----------------------------------------------------------------- |
| Resource and access-token audience | `https://arrpi.example.com/v1/mcp`                                |
| Required scope                     | `arrpi:mcp`, or your configured scope                             |
| Access-token header                | `typ: at+jwt` or `application/at+jwt`                             |
| Signing                            | An asymmetric algorithm with public keys in the server's JWKS     |
| Access-token lifetime              | At most one hour                                                  |
| Subject                            | A stable user identifier, not a mutable email address or username |

The server must publish OAuth or OIDC discovery metadata with its exact issuer,
JWKS location, authorization/token endpoints and `code_challenge_methods_supported`
including `S256`. Register the MCP client with its actual redirect URIs, or use the
authorization server's supported client-registration mechanism. Public clients
must not depend on a client secret. Clients send the canonical resource URL during
authorization and token exchange, as required by the
[MCP authorization specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization).

ArrPi requires valid `iss`, `aud`, `sub`, `exp` and `iat` claims, checks `nbf` when
present, and requires the configured scope. It allows 15 seconds of clock skew and
refuses tokens issued more than one hour ago. ID tokens, HMAC signatures and tokens
issued for another resource are rejected. Keep clocks synchronized.

HTTPS is required for remote issuers. HTTP issuers are accepted only on loopback
for local testing. Private certificate authorities must be trusted by the core
container's system certificate store.

### Authentik compatibility

The ordinary Authentik OAuth provider issues `typ: JWT` access tokens with its
OAuth client ID as the audience. It does not implement resource-URL targeting for
the authorization-code flow. Those tokens do not meet ArrPi's strict access-token
profile. OAuth remains disabled by default; existing per-account ArrPi tokens
continue to work with Authentik protecting the web interface.

Do not disable audience or token-type checks to accept a general login token. Use
an authorization server configured to issue the access-token profile above. The
[Authentik provider documentation](https://docs.goauthentik.io/add-secure-apps/providers/oauth2/)
describes its discovery, signing and subject settings; its
[token-exchange documentation](https://docs.goauthentik.io/add-secure-apps/providers/oauth2/token_exchange/)
also documents the unsupported `resource` parameter for that separate grant.

## Bind accounts in ArrPi

1. Configure Seerr and the public HTTPS MCP endpoint, and enable MCP.
2. In **Settings → Access**, create a normal requester with the correct Seerr and
   Plex IDs and the permissions it needs. OAuth cannot bind an `admin` or
   `config:manage` client.
3. In **Settings → OAuth**, enter the exact issuer from discovery, including its
   trailing slash when present. Set the required scope.
4. Add a binding from the issuer's exact `sub` value to that requester. Enable OAuth
   and save. Empty bindings grant access to nobody.

Bindings live in the same SQLite settings document as the account and its grants.
No email, username, group or role claim creates or upgrades an account. Each MCP
call retains the account's local grants and fresh Seerr/Plex identity checks.
Removing a binding denies subsequent requests even while the access token remains
valid. Explicitly moving a binding changes which local account that identity uses.
Existing requests drain under the settings revision that accepted them.

Usage and write history are attributed to the local ArrPi account. Bind different
people to separate accounts; identities mapped to one account share its grants and
history.

The CLI settings API accepts the same document. The optional root `oauth` section
has this shape:

```json
{
  "enabled": true,
  "issuer": "https://identity.example.com/realms/media",
  "required_scope": "arrpi:mcp",
  "bindings": [
    {
      "issuer": "https://identity.example.com/realms/media",
      "subject": "stable-user-id",
      "client": "alice"
    }
  ]
}
```

Changing an issuer does not silently rebind existing subjects: update the explicit
bindings deliberately. Removing a referenced client also requires removing its
bindings. OAuth changes apply when settings are saved.

## Client discovery and routing

Clients connect to the public `/v1/mcp` endpoint and follow its `401`
`WWW-Authenticate` challenge. ArrPi serves public
[RFC 9728](https://www.rfc-editor.org/rfc/rfc9728.html) metadata at:

```text
/.well-known/oauth-protected-resource/v1/mcp
/.well-known/oauth-protected-resource
```

Allow those exact paths through the reverse proxy without a browser sign-in
redirect. Keep the MCP endpoint token-authenticated. OAuth does not authorize the
configuration API, native read routes, metrics or calendar subscriptions.

A missing, invalid or expired token returns `401` with discovery information.
A valid token missing the required scope returns `403` with an
`insufficient_scope` challenge. A valid but unbound identity returns an application
`403` without asking the client to repeat authorization. Add or correct the binding
in Settings in that case.

Signing keys refresh in the background and when an unknown key is encountered,
with bounded fetches and a refresh cooldown. Unrelated settings changes preserve
cached signing keys; account bindings still apply from the new revision. Enable
`arrpi_adapters=debug` logging to inspect refusal reason codes without token data.
A browser logout at the authorization
server does not instantly revoke a signed access token; short lifetimes and ArrPi
binding removal provide the corresponding controls.

Existing account-token install commands and stdio remain available. OAuth clients
use their authorization server's registration and login flow rather than copying
an ArrPi token into an Authorization header.
