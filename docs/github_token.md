Creating a GitHub Personal Access Token
----------------------------------------

Step-by-step instructions:

1. Click on the top-right user icon, pick `⚙️ Settings` from the list (lower middle)

2. From the left menu, scroll to the very bottom, pick the last entry: `<> Developer settings`

3. From the left menu, expand `Personal access tokens` and click `Fine-grained tokens`

4. To the right of the page header, click the `Generate new token` button

5. GitHub may ask for multi-factor authentication

6. In `Token name` put `esdiag` or something descriptive

7. For `Resource owner`, pick the `elastic` organization

8. For `Expiration` use 90 days or custom; InfoSec recommends no more than 1 year

9. For `Repository Access`, pick `Only selected repositires`

10. In the `Select repositories` search box, type `esdiag` and pick two results: `esdiag` and `esdiag-dashboards`

11. Expand `Repository Permissions` and find `Contents`, update `Access: No access` to `Access: Read-only`

12. Scroll to the bottom and click `Generate token`

Now copy the token and save it somewhere safe, you will have to regenerate it if you lose it.

For this repository, add it to the root `.env` file:

```sh
export GITHUB_TOKEN="github_pat_1234567890abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
```
