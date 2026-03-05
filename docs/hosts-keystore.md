# Host Keystore Migration

`esdiag` supports two host auth models:

- Legacy plaintext credentials in `hosts.yml`
- Secret references from `hosts.yml` into an encrypted keystore

The keystore flow is optional and additive. Existing `hosts.yml` entries continue to work.

## Set Keystore Password

Set the keystore password in your shell before keystore operations:

```bash
export ESDIAG_KEYSTORE_PASSWORD="change-me"
```

## Add Secrets

Add a basic-auth secret:

```bash
esdiag keystore add prod-es-basic --user elastic --password changeme
```

Add an API key secret:

```bash
esdiag keystore add prod-es-apikey --apikey BASE64_ENCODED_KEY
```

## Reference Secrets from hosts.yml

Use `--secret` when creating/updating hosts:

```bash
esdiag host prod-es elasticsearch http://localhost:9200 --secret prod-es-apikey
```

The stored host entry keeps `secret` and omits plaintext auth values.

You can combine secret references with host roles:

```bash
esdiag host prod-es elasticsearch http://localhost:9200 --secret prod-es-apikey --roles collect,send
```

Role values are `collect`, `send`, and `view`.

## Migrate Existing hosts.yml

Migrate all legacy plaintext credentials into the keystore:

```bash
esdiag keystore migrate
```

Migration behavior:

- Uses each host name as `secret_id`
- Copies legacy `apikey` or `username/password` into keystore
- Rewrites host entries with `secret: <hostname>`
- Leaves hosts without legacy auth unchanged

## Optional Legacy Mode

If you do not use keystore secrets, keep legacy auth fields in `hosts.yml`.
`esdiag` continues to resolve auth from plaintext fields.

## Related CLI Arguments

- `esdiag host --secret <secret_id>` stores a secret reference instead of plaintext credentials
- `esdiag host --roles collect,send,view` assigns workflow roles to a host
- `esdiag host --user <name> --password <value>` stores legacy basic auth fields (alias: `--username`)
- `esdiag host --apikey <value>` stores a legacy API key field
- `esdiag keystore add/remove <secret_id> --user/--password/--apikey` manages encrypted secret entries
