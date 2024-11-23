"use strict";(self.webpackChunkdocs=self.webpackChunkdocs||[]).push([[2730],{5672:(e,n,s)=>{s.r(n),s.d(n,{assets:()=>c,contentTitle:()=>o,default:()=>p,frontMatter:()=>l,metadata:()=>t,toc:()=>h});const t=JSON.parse('{"id":"github/output","title":"Output","description":"When the action runs with command","source":"@site/docs/github/output.md","sourceDirName":"github","slug":"/github/output","permalink":"/docs/github/output","draft":false,"unlisted":false,"editUrl":"https://github.com/release-plz/release-plz/tree/main/website/docs/github/output.md","tags":[],"version":"current","frontMatter":{},"sidebar":"tutorialSidebar","previous":{"title":"Input variables","permalink":"/docs/github/input"},"next":{"title":"GitHub token","permalink":"/docs/github/token"}}');var r=s(4848),a=s(8453);const l={},o="Output",c={},h=[{value:"Example: read the output",id:"example-read-the-output",level:2},{value:"Example: add labels to released PRs",id:"example-add-labels-to-released-prs",level:2},{value:"Example: commit files to the release PR",id:"example-commit-files-to-the-release-pr",level:2}];function i(e){const n={a:"a",admonition:"admonition",code:"code",em:"em",h1:"h1",h2:"h2",header:"header",li:"li",p:"p",pre:"pre",ul:"ul",...(0,a.R)(),...e.components};return(0,r.jsxs)(r.Fragment,{children:[(0,r.jsx)(n.header,{children:(0,r.jsx)(n.h1,{id:"output",children:"Output"})}),"\n",(0,r.jsxs)(n.p,{children:["When the action runs with ",(0,r.jsx)(n.code,{children:"command: release-pr"}),", it outputs the following properties:"]}),"\n",(0,r.jsxs)(n.ul,{children:["\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"prs"}),": The release PRs opened by release-plz.\nIt's an array of objects with the properties of ",(0,r.jsx)(n.code,{children:"pr"}),".\n",(0,r.jsxs)(n.em,{children:["(Not useful for now. Use ",(0,r.jsx)(n.code,{children:"pr"})," instead)"]}),"."]}),"\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"pr"}),": The release PR opened by release-plz.\nIt's a JSON object with the following properties:","\n",(0,r.jsxs)(n.ul,{children:["\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"head_branch"}),": The name of the branch where the changes are implemented."]}),"\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"base_branch"}),": The name of the branch the changes are pulled into.\nIt is the default branch of the repository. E.g. ",(0,r.jsx)(n.code,{children:"main"}),"."]}),"\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"html_url"}),": The URL of the PR."]}),"\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"number"}),": The number of the PR."]}),"\n"]}),"\n"]}),"\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"prs_created"}),": Whether release-plz created any release PR. ",(0,r.jsx)(n.em,{children:"Boolean."})]}),"\n"]}),"\n",(0,r.jsxs)(n.p,{children:["When the action runs with ",(0,r.jsx)(n.code,{children:"command: release"}),", it outputs the following properties:"]}),"\n",(0,r.jsxs)(n.ul,{children:["\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"releases"}),": The JSON output of the ",(0,r.jsx)(n.code,{children:"release"})," command.\nIt's an array of JSON objects with the following properties:","\n",(0,r.jsxs)(n.ul,{children:["\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"package_name"}),": The name of the package that was released."]}),"\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"prs"}),": Array of PRs present in the changelog body of the release.\nUsually, they are the PRs containing the changes that were released.\nEach entry is an object containing:","\n",(0,r.jsxs)(n.ul,{children:["\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"html_url"}),": The URL of the PR."]}),"\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"number"}),": The number of the PR."]}),"\n"]}),"\n"]}),"\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"tag"}),": git tag name of the package that was released. It's returned even if you have\n",(0,r.jsx)(n.a,{href:"/docs/config#the-git_tag_enable-field",children:"git_tag_enable"})," set to ",(0,r.jsx)(n.code,{children:"false"}),", so that\nyou can use this to create the git tag yourself."]}),"\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"version"}),": The version of the package that was released."]}),"\n"]}),"\n"]}),"\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"releases_created"}),": Whether release-plz released any package. ",(0,r.jsx)(n.em,{children:"Boolean."})]}),"\n"]}),"\n",(0,r.jsx)(n.h2,{id:"example-read-the-output",children:"Example: read the output"}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-yaml",children:'jobs:\n\n  release-plz-release:\n    name: Release-plz release\n    runs-on: ubuntu-latest\n    steps:\n      - name: Checkout repository\n        uses: actions/checkout@v4\n        with:\n          fetch-depth: 0\n      - name: Install Rust toolchain\n        uses: dtolnay/rust-toolchain@stable\n      - name: Run release-plz\n# highlight-next-line\n        id: release-plz # <--- ID used to refer to the outputs. Don\'t forget it.\n        uses: release-plz/action@v0.5\n        with:\n          command: release\n        env:\n          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}\n      - name: Read release output\n        env:\n          RELEASES: ${{ steps.release-plz.outputs.releases }}\n          RELEASES_CREATED: ${{ steps.release-plz.outputs.releases_created }}\n        run: |\n          set -e\n          echo "releases: $RELEASES" # example: [{"package_name":"my-package","prs":[{"html_url":"https://github.com/user/proj/pull/1439","number":1439}],"tag":"v0.1.0","version":"0.1.0"}]\n          echo "releases_created: $RELEASES_CREATED" # example: true\n\n          # get the number of releases with jq\n          releases_length=$(echo "$RELEASES" | jq \'length\')\n          echo "releases_length: $releases_length"\n\n          # access the first release with jq\n          release_version=$(echo "$RELEASES" | jq -r \'.[0].version\')\n          echo "release_version: $release_version"\n\n          # access the first release with fromJSON. Docs: https://docs.github.com/en/actions/learn-github-actions/expressions\n          echo "release_version: ${{ fromJSON(steps.release-plz.outputs.releases)[0].version }}"\n\n          release_tag=$(echo "$RELEASES" | jq -r \'.[0].tag\')\n          echo "release_tag: $release_tag"\n\n          release_package_name=$(echo "$RELEASES" | jq -r \'.[0].package_name\')\n          echo "release_package_name: $release_package_name"\n\n          # print all names of released packages, one per line\n          echo "package_names: $(echo "$RELEASES" | jq -r \'.[].package_name\')"\n          # TODO: show how to store this in a variable and iterate over it (maybe an array?). PR welcome!\n\n          # iterate over released packages\n          for package_name in $(echo "$RELEASES" | jq -r \'.[].package_name\'); do\n              echo "released $package_name"\n          done\n\n  release-plz-pr:\n    name: Release-plz PR\n    runs-on: ubuntu-latest\n    concurrency:\n      group: release-plz-${{ github.ref }}\n      cancel-in-progress: false\n    steps:\n      - name: Checkout repository\n        uses: actions/checkout@v4\n        with:\n          fetch-depth: 0\n      - name: Install Rust toolchain\n        uses: dtolnay/rust-toolchain@stable\n      - name: Run release-plz\n# highlight-next-line\n        id: release-plz # <--- ID used to refer to the outputs. Don\'t forget it.\n        uses: release-plz/action@v0.5\n        with:\n          command: release-pr\n        env:\n          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}\n      - name: Read release-pr output\n        env:\n          PRS: ${{ steps.release-plz.outputs.prs }}\n          PR: ${{ steps.release-plz.outputs.pr }}\n          PRS_CREATED: ${{ steps.release-plz.outputs.prs_created }}\n        run: |\n          set -e\n          echo "prs: $PRS" # example: [{"base_branch":"main","head_branch":"release-plz-2024-05-01T20-38-05Z","html_url":"https://github.com/MarcoIeni/rust-workspace-example/pull/198","number":198}]\n          echo "pr: $PR" # example: {"base_branch":"main","head_branch":"release-plz-2024-05-01T20-38-05Z","html_url":"https://github.com/MarcoIeni/rust-workspace-example/pull/198","number":198}\n          echo "prs_created: $PRS_CREATED" # example: true\n\n          echo "pr_number: ${{ fromJSON(steps.release-plz.outputs.pr).number }}"\n          echo "pr_html_url: ${{ fromJSON(steps.release-plz.outputs.pr).html_url }}"\n          echo "pr_head_branch: ${{ fromJSON(steps.release-plz.outputs.pr).head_branch }}"\n          echo "pr_base_branch: ${{ fromJSON(steps.release-plz.outputs.pr).base_branch }}"\n'})}),"\n",(0,r.jsx)(n.h2,{id:"example-add-labels-to-released-prs",children:"Example: add labels to released PRs"}),"\n",(0,r.jsx)(n.p,{children:"It often happens, when looking for a feature or a bug fix, to land on a merged PR.\nThe next question: was this released? In what version?"}),"\n",(0,r.jsx)(n.p,{children:"With release-plz you can add a label to the PRs with the version they were released in:"}),"\n",(0,r.jsx)(n.admonition,{type:"info",children:(0,r.jsxs)(n.p,{children:["In this example, we are talking about the PRs containing code changes.\nWe aren't talking about the release PRs created by release-plz.\nYou can label release PRs with the ",(0,r.jsx)(n.a,{href:"/docs/config#the-pr_labels-field",children:"pr_labels"}),"\nconfiguration field."]})}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-yaml",children:'jobs:\n  release-plz-pr:\n    runs-on: ubuntu-latest\n    concurrency:\n      group: release-plz-${{ github.ref }}\n      cancel-in-progress: false\n    steps:\n      - name: Checkout repository\n        uses: actions/checkout@v4\n        with:\n          fetch-depth: 0\n      - name: Install Rust toolchain\n        uses: dtolnay/rust-toolchain@stable\n      - name: Run release-plz\n# highlight-next-line\n        id: release-plz # <--- ID used to refer to the outputs. Don\'t forget it.\n        uses: release-plz/action@v0.5\n        with:\n          command: release\n        env:\n          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}\n      - name: Tag released PRs\n        env:\n          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n          RELEASES: ${{ steps.release-plz.outputs.releases }}\n        run: |\n          set -e\n\n          # Iterate over released packages and add a label to the PRs\n          # shipped with the release.\n          for release in $(echo "$RELEASES" | jq -r -c \'.[]\'); do\n              package_name=$(echo "$release" | jq -r \'.package_name\')\n              version=$(echo "$release" | jq -r \'.version\')\n              prs_length=$(echo "$release" | jq \'.prs | length\')\n              if [ "$prs_length" -gt 0 ]; then\n                  # Create label.\n                  # Use `--force` to overwrite the label,\n                  # so that the command does not fail if the label already exists.\n                  label="released:$package_name-$version"\n                  echo "Creating label $label"\n                  gh label create $label --color BFD4F2 --force\n                  for pr in $(echo "$release" | jq -r -c \'.prs[]\'); do\n                      pr_number=$(echo "$pr" | jq -r \'.number\')\n                      echo "Adding label $label to PR #$pr_number"\n                      gh pr edit $pr_number --add-label $label\n                  done\n              else\n                  echo "No PRs found for package $package_name"\n              fi\n          done\n'})}),"\n",(0,r.jsxs)(n.p,{children:["You can also add a milestone with ",(0,r.jsx)(n.code,{children:"gh pr edit $pr_number --milestone <MILESTONE_NUMBER>"}),"."]}),"\n",(0,r.jsx)(n.admonition,{type:"tip",children:(0,r.jsx)(n.p,{children:"Make sure your GitHub token has permission to do all the operations you need."})}),"\n",(0,r.jsx)(n.h2,{id:"example-commit-files-to-the-release-pr",children:"Example: commit files to the release PR"}),"\n",(0,r.jsx)(n.p,{children:"You can commit files to the release PR opened by release-plz."}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-yaml",children:'jobs:\n  release-plz-pr:\n    runs-on: ubuntu-latest\n    concurrency:\n      group: release-plz-${{ github.ref }}\n      cancel-in-progress: false\n    steps:\n      - name: Checkout repository\n        uses: actions/checkout@v4\n        with:\n          fetch-depth: 0\n      - name: Install Rust toolchain\n        uses: dtolnay/rust-toolchain@stable\n      - name: Run release-plz\n# highlight-next-line\n        id: release-plz # <--- ID used to refer to the outputs. Don\'t forget it.\n        uses: release-plz/action@v0.5\n        with:\n          command: release-pr\n        env:\n          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}\n      - name: Update README in the release PR\n        env:\n          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n          PR: ${{ steps.release-plz.outputs.pr }}\n        run: |\n          set -e\n\n          pr_number=${{ fromJSON(steps.release-plz.outputs.pr).number }}\n          if [[ -n "$pr_number" ]]; then\n            gh pr checkout $pr_number\n            # change "echo" with your commands\n            echo "new readme" > README.md\n            git add .\n            git commit -m "Update README"\n            git push\n          fi\n'})})]})}function p(e={}){const{wrapper:n}={...(0,a.R)(),...e.components};return n?(0,r.jsx)(n,{...e,children:(0,r.jsx)(i,{...e})}):i(e)}},8453:(e,n,s)=>{s.d(n,{R:()=>l,x:()=>o});var t=s(6540);const r={},a=t.createContext(r);function l(e){const n=t.useContext(a);return t.useMemo((function(){return"function"==typeof e?e(n):{...n,...e}}),[n,e])}function o(e){let n;return n=e.disableParentContext?"function"==typeof e.components?e.components(r):e.components||r:l(e.components),t.createElement(a.Provider,{value:n},e.children)}}}]);