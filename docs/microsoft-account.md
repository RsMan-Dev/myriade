# Online-mode verification on the server

Horizon's dedicated server verifies players when `online-mode=true`, as vanilla 26.3 does.

`HORIZON_API_DISCOVERY` overrides the discovery URL, the counterpart of authlib's
`-Dminecraft.api.discovery.host`; it is how the harness points both the vanilla server and Horizon at
a local mock.
