"use strict";(self.webpackChunkdocs=self.webpackChunkdocs||[]).push([[2859],{7884:(e,n,s)=>{s.r(n),s.d(n,{assets:()=>a,contentTitle:()=>l,default:()=>u,frontMatter:()=>i,metadata:()=>t,toc:()=>c});const t=JSON.parse('{"id":"github/quickstart","title":"Quickstart","description":"This guide shows how to run the release-plz","source":"@site/docs/github/quickstart.md","sourceDirName":"github","slug":"/github/quickstart","permalink":"/docs/github/quickstart","draft":false,"unlisted":false,"editUrl":"https://github.com/release-plz/release-plz/tree/main/website/docs/github/quickstart.md","tags":[],"version":"current","frontMatter":{},"sidebar":"tutorialSidebar","previous":{"title":"GitHub Action","permalink":"/docs/github/"},"next":{"title":"Input variables","permalink":"/docs/github/input"}}');var r=s(4848),o=s(8453);const i={},l="Quickstart",a={},c=[{value:"1. Change GitHub Actions permissions",id:"1-change-github-actions-permissions",level:2},{value:"2. Set the <code>CARGO_REGISTRY_TOKEN</code> secret",id:"2-set-the-cargo_registry_token-secret",level:2},{value:"3. Setup the workflow",id:"3-setup-the-workflow",level:2},{value:"Concurrency",id:"concurrency",level:3}];function h(e){const n={a:"a",code:"code",h1:"h1",h2:"h2",h3:"h3",header:"header",img:"img",li:"li",ol:"ol",p:"p",pre:"pre",ul:"ul",...(0,o.R)(),...e.components},{Details:t}=n;return t||function(e,n){throw new Error("Expected "+(n?"component":"object")+" `"+e+"` to be defined: you likely forgot to import, pass, or provide it.")}("Details",!0),(0,r.jsxs)(r.Fragment,{children:[(0,r.jsx)(n.header,{children:(0,r.jsx)(n.h1,{id:"quickstart",children:"Quickstart"})}),"\n",(0,r.jsxs)(n.p,{children:["This guide shows how to run the release-plz\n",(0,r.jsx)(n.a,{href:"https://github.com/marketplace/actions/release-plz",children:"GitHub Action"}),"\nevery time you merge a commit to the main branch.\nThe workflow will have two jobs, running the following commands:"]}),"\n",(0,r.jsxs)(n.ul,{children:["\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.a,{href:"/docs/usage/release-pr",children:(0,r.jsx)(n.code,{children:"release-plz release-pr"})}),": creates the release pr."]}),"\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.a,{href:"/docs/usage/release",children:(0,r.jsx)(n.code,{children:"release-plz release"})}),": publishes the unpublished packages."]}),"\n"]}),"\n",(0,r.jsx)(n.p,{children:"Follow the steps below to set up the GitHub Action."}),"\n",(0,r.jsx)(n.h2,{id:"1-change-github-actions-permissions",children:"1. Change GitHub Actions permissions"}),"\n",(0,r.jsxs)(n.ol,{children:["\n",(0,r.jsxs)(n.li,{children:["\n",(0,r.jsx)(n.p,{children:"Go to the GitHub Actions settings:"}),"\n",(0,r.jsx)(n.p,{children:(0,r.jsx)(n.img,{alt:"actions settings",src:s(9597).A+"",width:"1484",height:"1212"})}),"\n"]}),"\n",(0,r.jsxs)(n.li,{children:["\n",(0,r.jsx)(n.p,{children:'Change "Workflow permissions" to allow GitHub Actions to create and approve\npull requests (needed to create and update the PR).'}),"\n",(0,r.jsx)(n.p,{children:(0,r.jsx)(n.img,{alt:"workflow permission",src:s(2084).A+"",width:"1876",height:"634"})}),"\n"]}),"\n"]}),"\n",(0,r.jsxs)(n.h2,{id:"2-set-the-cargo_registry_token-secret",children:["2. Set the ",(0,r.jsx)(n.code,{children:"CARGO_REGISTRY_TOKEN"})," secret"]}),"\n",(0,r.jsx)(n.p,{children:"Release-plz needs a token to publish your packages to the cargo registry."}),"\n",(0,r.jsxs)(n.ol,{children:["\n",(0,r.jsxs)(n.li,{children:["Retrieve your registry token following\n",(0,r.jsx)(n.a,{href:"https://doc.rust-lang.org/cargo/reference/publishing.html#before-your-first-publish",children:"this"}),"\nguide."]}),"\n",(0,r.jsxs)(n.li,{children:["Add your cargo registry token as a secret in your repository following\n",(0,r.jsx)(n.a,{href:"https://docs.github.com/en/actions/security-guides/encrypted-secrets#creating-encrypted-secrets-for-a-repository",children:"this"}),"\nguide."]}),"\n"]}),"\n",(0,r.jsxs)(n.p,{children:["As specified in the ",(0,r.jsx)(n.code,{children:"cargo publish"}),"\n",(0,r.jsx)(n.a,{href:"https://doc.rust-lang.org/cargo/commands/cargo-publish.html#publish-options",children:"options"}),":"]}),"\n",(0,r.jsxs)(n.ul,{children:["\n",(0,r.jsxs)(n.li,{children:["The token for ",(0,r.jsx)(n.a,{href:"https://crates.io/",children:"crates.io"})," shall be specified with the ",(0,r.jsx)(n.code,{children:"CARGO_REGISTRY_TOKEN"}),"\nenvironment variable."]}),"\n",(0,r.jsxs)(n.li,{children:["Tokens for other registries shall be specified with environment variables of the form\n",(0,r.jsx)(n.code,{children:"CARGO_REGISTRIES_NAME_TOKEN"})," where ",(0,r.jsx)(n.code,{children:"NAME"})," is the name of the registry in all capital letters."]}),"\n"]}),"\n",(0,r.jsxs)(n.p,{children:["If you are creating a new crates.io token, specify the scopes ",(0,r.jsx)(n.code,{children:"publish-new"})," and ",(0,r.jsx)(n.code,{children:"publish-update"}),":"]}),"\n",(0,r.jsx)(n.p,{children:(0,r.jsx)(n.img,{alt:"token scope",src:s(6374).A+"",width:"1442",height:"460"})}),"\n",(0,r.jsx)(n.h2,{id:"3-setup-the-workflow",children:"3. Setup the workflow"}),"\n",(0,r.jsxs)(n.p,{children:["Create the release-plz workflow file under the ",(0,r.jsx)(n.code,{children:".github/workflows"})," directory\n(for example ",(0,r.jsx)(n.code,{children:".github/workflows/release-plz.yml"}),")\nand copy the following workflow:"]}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-yaml",children:"name: Release-plz\n\npermissions:\n  pull-requests: write\n  contents: write\n\non:\n  push:\n    branches:\n      - main\n\njobs:\n\n  # Release unpublished packages.\n  release-plz-release:\n    name: Release-plz release\n    runs-on: ubuntu-latest\n    permissions:\n      contents: write\n    steps:\n      - name: Checkout repository\n        uses: actions/checkout@v4\n        with:\n          fetch-depth: 0\n      - name: Install Rust toolchain\n        uses: dtolnay/rust-toolchain@stable\n      - name: Run release-plz\n        uses: release-plz/action@v0.5\n        with:\n          command: release\n        env:\n          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}\n\n  # Create a PR with the new versions and changelog, preparing the next release.\n  release-plz-pr:\n    name: Release-plz PR\n    runs-on: ubuntu-latest\n    permissions:\n      contents: write\n      pull-requests: write\n    concurrency:\n      group: release-plz-${{ github.ref }}\n      cancel-in-progress: false\n    steps:\n      - name: Checkout repository\n        uses: actions/checkout@v4\n        with:\n          fetch-depth: 0\n      - name: Install Rust toolchain\n        uses: dtolnay/rust-toolchain@stable\n      - name: Run release-plz\n        uses: release-plz/action@v0.5\n        with:\n          command: release-pr\n        env:\n          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}\n"})}),"\n",(0,r.jsxs)(t,{children:[(0,r.jsx)("summary",{children:"Workflow explanation"}),(0,r.jsx)(n.p,{children:"This optional section adds comments to the above workflow,\nto explain it in detail."}),(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-yaml",children:"# Name of the workflow: you can change it.\nname: Release-plz\n\n# The action runs on every push to the main branch.\non:\n  push:\n    branches:\n      - main\n\njobs:\n\n  # Release unpublished packages.\n  # If you want release-plz to only update your packages,\n  # and you want to handle `cargo publish` and git tag push by yourself,\n  # remove this job.\n  release-plz-release:\n    name: Release-plz release\n    runs-on: ubuntu-latest\n    # Used to push tags, and create releases.\n    permissions:\n      contents: write\n    steps:\n      - name: Checkout repository\n        uses: actions/checkout@v4\n        with:\n          # `fetch-depth: 0` is needed to clone all the git history, which is necessary to\n          # release from the latest commit of the release PR.\n          fetch-depth: 0\n      # Use your favorite way to install the Rust toolchain.\n      # The action I'm using here is a popular choice.\n      - name: Install Rust toolchain\n        uses: dtolnay/rust-toolchain@stable\n      - name: Run release-plz\n        uses: release-plz/action@v0.5\n        with:\n          # Run `release-plz release` command.\n          command: release\n        env:\n          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}\n\n  # Create a PR with the new versions and changelog, preparing the next release.\n  # If you want release-plz to only release your packages\n  # and you want to update `Cargo.toml` versions and changelogs by yourself,\n  # remove this job.\n  release-plz-pr:\n    name: Release-plz PR\n    runs-on: ubuntu-latest\n    permissions:\n      # Used to create and update pull requests.\n      pull-requests: write\n      # Used to push to the pull request branch.\n      contents: write\n\n    # The concurrency block is explained below (after the code block).\n    concurrency:\n      group: release-plz-${{ github.ref }}\n      cancel-in-progress: false\n    steps:\n      - name: Checkout repository\n        uses: actions/checkout@v4\n        with:\n          # `fetch-depth: 0` is needed to clone all the git history, which is necessary to\n          # determine the next version and build the changelog.\n          fetch-depth: 0\n      - name: Install Rust toolchain\n        uses: dtolnay/rust-toolchain@stable\n      - name: Run release-plz\n        uses: release-plz/action@v0.5\n        with:\n          # Run `release-plz release-pr` command.\n          command: release-pr\n        env:\n          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n          # In `release-plz-pr` this is only required if you are using a private registry.\n          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}\n"})}),(0,r.jsx)(n.h3,{id:"concurrency",children:"Concurrency"}),(0,r.jsxs)(n.p,{children:["The ",(0,r.jsx)(n.code,{children:"concurrency"})," block guarantees that if a new commit is pushed while\nthe job of the previous commit was still running, the new job will\nwait for the previous one to finish.\nIn this way, only one instance of ",(0,r.jsx)(n.code,{children:"release-plz release-pr"})," will run in the\nrepository at the same time for # the same branch, ensuring that there are\nno conflicts.\nSee the GitHub ",(0,r.jsx)(n.a,{href:"https://docs.github.com/en/actions/writing-workflows/workflow-syntax-for-github-actions#jobsjob_idconcurrency",children:"docs"}),"\nto learn more."]}),(0,r.jsxs)(n.p,{children:["We can't use the same ",(0,r.jsx)(n.code,{children:"concurrency"})," block in the ",(0,r.jsx)(n.code,{children:"release-plz-release"})," job\nbecause the ",(0,r.jsx)(n.code,{children:"concurrency"})," block cancels the pending job if a new commit is\npushed \u2014 we can't risk to skip a release.\nThis is an example commit sequence where the release would be skipped:"]}),(0,r.jsxs)(n.ul,{children:["\n",(0,r.jsxs)(n.li,{children:["Commit 1: an initial commit is pushed to the main branch. ",(0,r.jsx)(n.code,{children:"release-plz release"})," runs."]}),"\n",(0,r.jsx)(n.li,{children:"Commit 2: a second commit is pushed to the main branch. The job of this commit is pending,\nwaiting for Release-plz to finish on Commit 1."}),"\n",(0,r.jsx)(n.li,{children:"Commit 3: a third commit is pushed to the main branch. The job of commit 2 is canceled,\nand the job of commit 3 is pending, waiting for Release-plz to finish on Commit 1."}),"\n"]})]})]})}function u(e={}){const{wrapper:n}={...(0,o.R)(),...e.components};return n?(0,r.jsx)(n,{...e,children:(0,r.jsx)(h,{...e})}):h(e)}},9597:(e,n,s)=>{s.d(n,{A:()=>t});const t=s.p+"assets/images/actions_settings-29f01e7f00f3c53f1aef4ccc0689b483.png"},6374:(e,n,s)=>{s.d(n,{A:()=>t});const t=s.p+"assets/images/token_scope-5d0da8f1b61e22bb12823c49db0a3e81.png"},2084:(e,n,s)=>{s.d(n,{A:()=>t});const t=s.p+"assets/images/workflow_permissions-1b2139cf34240279ab7e14dcd3497b72.png"},8453:(e,n,s)=>{s.d(n,{R:()=>i,x:()=>l});var t=s(6540);const r={},o=t.createContext(r);function i(e){const n=t.useContext(o);return t.useMemo((function(){return"function"==typeof e?e(n):{...n,...e}}),[n,e])}function l(e){let n;return n=e.disableParentContext?"function"==typeof e.components?e.components(r):e.components||r:i(e.components),t.createElement(o.Provider,{value:n},e.children)}}}]);