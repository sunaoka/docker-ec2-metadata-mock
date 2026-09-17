# EC2 IMDS Mock

An EC2 Instance Metadata Service (IMDS) mock for local development and testing. It runs as a Docker Compose sidecar that shares an application container's network namespace, allowing AWS SDKs and the AWS CLI to use the standard `http://169.254.169.254` endpoint. Do not use it in production.

## Usage

`imds` shares its network namespace with `app`. The sidecar adds `169.254.169.254/32` to loopback and creates an iptables NAT rule, so it requires `NET_ADMIN` and must run as root.

```yaml
services:
  app:
    image: example/app

  imds:
    image: sunaoka/ec2-metadata-mock:latest
    network_mode: service:app
    cap_add:
      - NET_ADMIN
```

The server listens on `127.0.0.1:8181` by default. An iptables `OUTPUT` rule redirects `169.254.169.254:80` to that listener. This allows the application container to keep using `0.0.0.0:80`.

Set `IMDS_IPV6_ENABLED=1` to also provide the AWS IMDS IPv6 endpoint at `http://[fd00:ec2::254]`. It adds the IPv6 address and an ip6tables NAT rule, then listens on `[::1]:8181`. Keep it disabled when the Docker runtime has IPv6 disabled.

When using both `php` and `php-testing`, create one sidecar for each application container.

```yaml
  php-imds:
    image: sunaoka/ec2-metadata-mock:latest
    network_mode: service:php
    cap_add:
      - NET_ADMIN

  php-testing-imds:
    image: sunaoka/ec2-metadata-mock:latest
    network_mode: service:php-testing
    cap_add:
      - NET_ADMIN
```

It works with Docker Desktop and Linux Docker Engine. The multi-stage Dockerfile supports buildx builds for `linux/amd64` and `linux/arm64`.

## IMDS API

- `PUT /latest/api/token` requires `X-aws-ec2-metadata-token-ttl-seconds` and accepts values from 1 to 21,600 seconds.
- `GET /latest/meta-data/iam/security-credentials/` returns the role name.
- `GET /latest/meta-data/iam/security-credentials/{role}` returns JSON compatible with the Instance Profile provider.
- `GET /health` is a mock-specific endpoint used by the Docker health check.

IMDSv2 tokens are required by default. Set `IMDS_V1_ENABLED=1` to permit token-free IMDSv1 requests. AWS supports both an IMDSv1-and-v2 mode and an IMDSv2-only mode; this local mock defaults to requiring tokens.

## Environment variables

| Variable                      | Default      | Description                                                   |
| ----------------------------- | ------------ | ------------------------------------------------------------- |
| `IMDS_ROLE_NAME`              | `local-role` | IAM role name                                                 |
| `AWS_ACCESS_KEY_ID`           | `test`       | Access key returned by the mock                               |
| `AWS_SECRET_ACCESS_KEY`       | `test`       | Secret access key returned by the mock                        |
| `AWS_SESSION_TOKEN`           | `test`       | Session token returned by the mock                            |
| `IMDS_CREDENTIAL_TTL_SECONDS` | `3600`       | Seconds until credential expiration                           |
| `IMDS_V1_ENABLED`             | `0`          | Permit token-free requests when set to `1`                    |
| `IMDS_IPV6_ENABLED`           | `0`          | Enable the `fd00:ec2::254` IMDS IPv6 endpoint when set to `1` |
| `IMDS_LISTEN_PORT`            | `8181`       | Loopback listener port; use 1 through 65535                   |
| `DEBUG`                       | `0`          | Enable debug-level IMDS request and response logs when `1`    |

`DEBUG=1` sets the default log filter to `debug`; `RUST_LOG` overrides that default when explicitly set. Existing server and IMDS endpoint logs remain at `INFO`. Docker health checks never produce debug request or response logs.

Access keys, secret access keys, session tokens, and IMDS tokens are never written to logs unless `DEBUG=1`. Debug logging writes all of them to Docker logs; use it only for local testing.

## Using with LocalStack

Do not explicitly configure credentials in the AWS SDK. The SDK retrieves credentials from IMDS, while AWS API requests go to the LocalStack endpoint.

```text
AWS SDK
  ├─ credential → 169.254.169.254 → IMDS mock
  └─ API request → LocalStack:4566
```

## Verification

```sh
make test
make build
```
