"use strict";(self.webpackChunkdocs=self.webpackChunkdocs||[]).push([[5372],{3045:(e,n,t)=>{t.r(n),t.d(n,{assets:()=>o,contentTitle:()=>l,default:()=>h,frontMatter:()=>c,metadata:()=>s,toc:()=>a});const s=JSON.parse('{"id":"github/input","title":"Input variables","description":"The GitHub action accepts the following input variables:","source":"@site/docs/github/input.md","sourceDirName":"github","slug":"/github/input","permalink":"/docs/github/input","draft":false,"unlisted":false,"editUrl":"https://github.com/release-plz/release-plz/tree/main/website/docs/github/input.md","tags":[],"version":"current","frontMatter":{},"sidebar":"tutorialSidebar","previous":{"title":"Quickstart","permalink":"/docs/github/quickstart"},"next":{"title":"Output","permalink":"/docs/github/output"}}');var i=t(4848),r=t(8453);const c={},l="Input variables",o={},a=[];function d(e){const n={code:"code",em:"em",h1:"h1",header:"header",li:"li",p:"p",pre:"pre",ul:"ul",...(0,r.R)(),...e.components};return(0,i.jsxs)(i.Fragment,{children:[(0,i.jsx)(n.header,{children:(0,i.jsx)(n.h1,{id:"input-variables",children:"Input variables"})}),"\n",(0,i.jsx)(n.p,{children:"The GitHub action accepts the following input variables:"}),"\n",(0,i.jsxs)(n.ul,{children:["\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"command"}),": The release-plz command to run. Accepted values: ",(0,i.jsx)(n.code,{children:"release-pr"}),",\n",(0,i.jsx)(n.code,{children:"release"}),". ",(0,i.jsx)(n.em,{children:"(By default it runs both commands)."})]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"registry"}),": Registry where the packages are stored.\nThe registry name needs to be present in the Cargo config.\nIf unspecified, the ",(0,i.jsx)(n.code,{children:"publish"})," field of the package manifest is used.\nIf the ",(0,i.jsx)(n.code,{children:"publish"})," field is empty, crates.io is used."]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"manifest_path"}),": Path to the Cargo.toml of the project you want to update.\nBoth Cargo workspaces and single packages are supported.\n",(0,i.jsx)(n.em,{children:"(Defaults to the root directory)."})]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"version"}),": Release-plz version to use. E.g. ",(0,i.jsx)(n.code,{children:"0.3.70"}),". ",(0,i.jsx)(n.em,{children:"(Default: latest version)."})]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"config"}),": Release-plz config file location.\n",(0,i.jsxs)(n.em,{children:["(Defaults to ",(0,i.jsx)(n.code,{children:"release-plz.toml"})," or ",(0,i.jsx)(n.code,{children:".release-plz.toml"}),")."]})]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"token"}),": Token used to publish to the cargo registry.\nOverride the ",(0,i.jsx)(n.code,{children:"CARGO_REGISTRY_TOKEN"})," environment variable, or the ",(0,i.jsx)(n.code,{children:"CARGO_REGISTRIES_<NAME>_TOKEN"}),"\nenvironment variable, used for registry specified in the ",(0,i.jsx)(n.code,{children:"registry"})," input variable."]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"backend"}),": Forge backend. Valid values: ",(0,i.jsx)(n.code,{children:"github"}),", ",(0,i.jsx)(n.code,{children:"gitea"}),". ",(0,i.jsxs)(n.em,{children:["(Defaults to ",(0,i.jsx)(n.code,{children:"github"}),")."]})]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"verbose"}),": Print module and source location in logs.\nI.e. adds the ",(0,i.jsx)(n.code,{children:"-v"})," flag to the command. ",(0,i.jsxs)(n.em,{children:["(Defaults to ",(0,i.jsx)(n.code,{children:"false"}),")."]})]}),"\n"]}),"\n",(0,i.jsxs)(n.p,{children:["You can specify the input variables by using the ",(0,i.jsx)(n.code,{children:"with"})," keyword.\nFor example:"]}),"\n",(0,i.jsx)(n.pre,{children:(0,i.jsx)(n.code,{className:"language-yaml",children:"steps:\n  - ...\n  - name: Run release-plz\n    uses: release-plz/action@v0.5\n# highlight-start\n    # Input variables\n    with:\n      command: release-pr\n      registry: my-registry\n      manifest_path: rust-crates/my-crate/Cargo.toml\n      version: 0.3.70\n# highlight-end\n    env:\n      GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n      CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}\n"})})]})}function h(e={}){const{wrapper:n}={...(0,r.R)(),...e.components};return n?(0,i.jsx)(n,{...e,children:(0,i.jsx)(d,{...e})}):d(e)}},8453:(e,n,t)=>{t.d(n,{R:()=>c,x:()=>l});var s=t(6540);const i={},r=s.createContext(i);function c(e){const n=s.useContext(r);return s.useMemo((function(){return"function"==typeof e?e(n):{...n,...e}}),[n,e])}function l(e){let n;return n=e.disableParentContext?"function"==typeof e.components?e.components(i):e.components||i:c(e.components),s.createElement(r.Provider,{value:n},e.children)}}}]);