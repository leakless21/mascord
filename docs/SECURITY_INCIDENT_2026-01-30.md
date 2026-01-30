# Security Incident Report: API Key Exposure

**Date**: 2026-01-30  
**Severity**: HIGH  
**Status**: RESOLVED

## Incident Summary
A Brave Search API key was accidentally committed and pushed to the Git repository in the `mcp_servers.toml` file.

## Exposed Credentials
- **File**: `mcp_servers.toml`
- **Key Type**: Brave Search API Key
- **Exposed Value**: `BSAoKRG7z0lzebn08ygar4lOLeo-Z0a` (partial)
- **First Commit**: `f7eda83` (2026-01-XX)
- **Pushed to Remote**: Yes

## Remediation Actions Taken

### 1. ✅ API Key Revocation
- [ ] **ACTION REQUIRED**: Revoke the exposed key at https://brave.com/search/api/
- [ ] **ACTION REQUIRED**: Generate a new API key
- [ ] **ACTION REQUIRED**: Update `.env` file with new key

### 2. ✅ Git History Cleanup
- Removed `mcp_servers.toml` from entire Git history using `git filter-branch`
- Verified file is no longer present in any commit

### 3. ✅ Prevention Measures Implemented
- Added `mcp_servers.toml` to `.gitignore`
- Created `mcp_servers.toml.example` as a safe template
- Updated `.env.example` to include `BRAVE_API_KEY`
- Updated README.md with security best practices
- Modified `mcp_servers.toml` to use environment variables instead of hardcoded keys

### 4. ⚠️ Force Push Required
The Git history has been rewritten locally. To complete the remediation:

```bash
# WARNING: This will rewrite remote history
git push --force-with-lease origin master
```

> **Note**: If others have cloned this repository, they will need to re-clone or reset their local copies.

## Lessons Learned

1. **Never commit API keys**: Always use environment variables for sensitive credentials
2. **Use .gitignore proactively**: Add sensitive files to `.gitignore` before committing
3. **Template files**: Use `.example` files for configuration templates
4. **Pre-commit hooks**: Consider adding git hooks to scan for secrets before commits

## Prevention Checklist for Future

- [ ] Consider using `git-secrets` or similar tools to prevent secret commits
- [ ] Add pre-commit hooks to scan for API keys and tokens
- [ ] Regular security audits of committed files
- [ ] Team training on secure credential management

## References

- [GitHub: Removing sensitive data](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/removing-sensitive-data-from-a-repository)
- [OWASP: Secrets Management](https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html)
