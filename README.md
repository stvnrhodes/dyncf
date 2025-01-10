# dyncf (DYNamic CloudFlare)

Fetch current ip addresses and update a record in cloudflare with them. Addresses are discovered via https://cloudflare.com/cdn-cgi/trace.

Run it with

```shell
go run . -dns-domain mysubdomain.example.com -cloudflare-api-token-file file-with-token
```
