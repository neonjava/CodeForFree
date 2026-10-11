# Security policy

This early version is intended for a single user on localhost. It has no login system or HTTPS and must not be bound to a public interface or deployed as a public web service. Conversations are stored in plaintext in the local SQLite file. API keys are read only from server environment variables and are not stored in the database or served to the browser.

Please report vulnerabilities through GitHub's private vulnerability reporting for this repository if available, rather than a public issue. Do not include live credentials in reports.
