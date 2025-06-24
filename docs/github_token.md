GitHub Personal Access Tokens
-----------------------------

The GitHub personal access token is a way to grant fine-grained access control to your GitHub account. It allows you to specify which actions you want to perform and which resources you want to access. GitHub personal access tokens are used to authenticate API requests and can be used to automate tasks such as creating and managing repositories, managing issues and pull requests, and more.

GitHub documentation: https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens

For ESDiag, the token is only used for read-only automated version checks to the private repositories.

### Generating a GitHub Personal Access Token

1. Click on the top-right user icon, pick `⚙️ Settings` from the list (lower middle)

2. From the left menu, scroll to the very bottom, pick the last entry: `<> Developer settings`

3. From the left menu, expand `Personal access tokens` and click `Fine-grained tokens`

4. To the right of the page header, click the `Generate new token` button

5. GitHub may ask for multi-factor authentication

6. In `Token name` put `esdiag` or something descriptive

7. For `Resource owner`, pick the `elastic` organization

8. For `Expiration` use 90 days or custom; InfoSec recommends no more than 1 year

9. For `Repository Access`, pick `Only selected repositories`

10. In the `Select repositories` search box, type `esdiag` and pick two results: `esdiag` and `esdiag-dashboards`

11. Expand `Repository Permissions` and find `Contents`, update `Access: No access` to `Access: Read-only`

12. Scroll to the bottom and click `Generate token`

Now copy the token and save it somewhere safe, you will have to regenerate it if you lose it.

For this repository, add it to the root `.env` file (see `example.env`):

```sh
export GITHUB_TOKEN="github_pat_1234567890abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
```
