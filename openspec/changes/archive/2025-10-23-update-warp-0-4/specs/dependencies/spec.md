## MODIFIED Requirements
### Requirement: Maintain HTTP Server Dependencies
- The project MUST target Warp 0.4.x (and associated hyper/tokio compatibility versions) for the embedded HTTP server.
- Server handlers MUST compile without deprecation warnings under the chosen Warp version and continue to expose the existing endpoints (`/`, `/feed.xml`, `/api/stars`).

#### Scenario: Warp Upgrade Retains Behaviour
1. Given the HTTP server is built with Warp 0.4
2. When the existing route tests (`/feed.xml`, `/`, `/api/stars`) are executed
3. Then they MUST pass without behavioural regressions compared to Warp 0.3
