"use strict";(self.webpackChunkdocs=self.webpackChunkdocs||[]).push([[1147],{5676:(e,n,a)=>{a.r(n),a.d(n,{assets:()=>s,contentTitle:()=>i,default:()=>g,frontMatter:()=>l,metadata:()=>t,toc:()=>r});const t=JSON.parse('{"id":"extra/single-changelog","title":"Single changelog","description":"One package","source":"@site/docs/extra/single-changelog.md","sourceDirName":"extra","slug":"/extra/single-changelog","permalink":"/docs/extra/single-changelog","draft":false,"unlisted":false,"editUrl":"https://github.com/release-plz/release-plz/tree/main/website/docs/extra/single-changelog.md","tags":[],"version":"current","frontMatter":{},"sidebar":"tutorialSidebar","previous":{"title":"Releasing binaries","permalink":"/docs/extra/releasing-binaries"},"next":{"title":"Single git tag","permalink":"/docs/extra/single-tag"}}');var o=a(4848),c=a(8453);const l={},i="Single changelog",s={},r=[{value:"One package",id:"one-package",level:2},{value:"All packages",id:"all-packages",level:2}];function h(e){const n={a:"a",admonition:"admonition",code:"code",h1:"h1",h2:"h2",header:"header",p:"p",pre:"pre",...(0,c.R)(),...e.components};return(0,o.jsxs)(o.Fragment,{children:[(0,o.jsx)(n.header,{children:(0,o.jsx)(n.h1,{id:"single-changelog",children:"Single changelog"})}),"\n",(0,o.jsx)(n.h2,{id:"one-package",children:"One package"}),"\n",(0,o.jsxs)(n.p,{children:["If you have a workspace with multiple packages, and you want to keep\ntrack of the changes of just one package, you can customize your\n",(0,o.jsx)(n.code,{children:"release-plz.toml"})," file like this:"]}),"\n",(0,o.jsx)(n.pre,{children:(0,o.jsx)(n.code,{className:"language-toml",children:'[workspace]\n# disable the changelog for all packages\nchangelog_update = false\n\n[[package]]\nname = "my-important-package"\n# enable the changelog for this package\nchangelog_update = true\n# set the path of the changelog to the root of the repository\nchangelog_path = "./CHANGELOG.md"\n'})}),"\n",(0,o.jsxs)(n.p,{children:["To include commits of other packages in the changelog of\nyour main package, use the\n",(0,o.jsx)(n.a,{href:"/docs/config#the-changelog_include-field",children:"changelog_include"})," field."]}),"\n",(0,o.jsx)(n.h2,{id:"all-packages",children:"All packages"}),"\n",(0,o.jsxs)(n.p,{children:["If you have a workspace with multiple packages, and you want to group all the\nchanges in a single changelog, you can customize your ",(0,o.jsx)(n.code,{children:"release-plz.toml"}),"\nfile like this:"]}),"\n",(0,o.jsx)(n.pre,{children:(0,o.jsx)(n.code,{className:"language-toml",children:'[workspace]\n# set the path of all the crates to the changelog to the root of the repository\nchangelog_path = "./CHANGELOG.md"\n\n[changelog]\nbody = """\n\n## `{{ package }}` - [{{ version }}]{%- if release_link -%}({{ release_link }}){% endif %} - {{ timestamp | date(format="%Y-%m-%d") }}\n{% for group, commits in commits | group_by(attribute="group") %}\n### {{ group | upper_first }}\n{% for commit in commits %}\n{%- if commit.scope -%}\n- *({{commit.scope}})* {% if commit.breaking %}[**breaking**] {% endif %}{{ commit.message }}{%- if commit.links %} ({% for link in commit.links %}[{{link.text}}]({{link.href}}) {% endfor -%}){% endif %}\n{% else -%}\n- {% if commit.breaking %}[**breaking**] {% endif %}{{ commit.message }}\n{% endif -%}\n{% endfor -%}\n{% endfor -%}\n"""\n'})}),"\n",(0,o.jsxs)(n.p,{children:["The difference with the default changelog body configuration is that the header now also contains\nthe ",(0,o.jsx)(n.code,{children:"{{package}}"}),"."]}),"\n",(0,o.jsxs)(n.p,{children:["The entries will be order depending on the release order: if ",(0,o.jsx)(n.code,{children:"package_a"})," depends on ",(0,o.jsx)(n.code,{children:"package_b"}),",\nthe changes of ",(0,o.jsx)(n.code,{children:"package_a"})," are listed after ",(0,o.jsx)(n.code,{children:"package_b"}),".\nFor example, if you have a workspace with various libraries and one binary that depends on all the\nlibraries, the changes of the binary will be placed first."]}),"\n",(0,o.jsxs)(n.p,{children:["In this way, ",(0,o.jsx)(n.code,{children:"package_a"})," and ",(0,o.jsx)(n.code,{children:"package_b"})," changelogs are in the same file.\nNote that the changelog will contain duplicate changes.\nIf you want to merge updates of different packages into one, check\nthe ",(0,o.jsx)(n.a,{href:"/docs/config#the-changelog_include-field",children:"changelog_include"})," field."]}),"\n",(0,o.jsx)(n.p,{children:"Here's an example of the changelog output:"}),"\n",(0,o.jsx)(n.pre,{children:(0,o.jsx)(n.code,{className:"language-md",children:"# Changelog\n\nAll notable changes to this project will be documented in this file.\n\nThe format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),\nand this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).\n\n## [Unreleased]\n\n## `marco-test-two` - [0.4.20](https://github.com/marcoieni/rust-workspace-example/compare/marco-test-two-v0.4.19...marco-test-two-v0.4.20) - 2024-05-17\n\n### Added\n- new feature\n\n## `marco-test-three` - [0.1.16](https://github.com/marcoieni/rust-workspace-example/compare/marco-test-three-v0.1.15...marco-test-three-v0.1.16) - 2024-05-17\n\n### Other\n- Small change\n"})}),"\n",(0,o.jsxs)(n.admonition,{type:"tip",children:[(0,o.jsx)(n.p,{children:"You can enable the changelog for a subset of packages only:"}),(0,o.jsx)(n.pre,{children:(0,o.jsx)(n.code,{className:"language-toml",children:'[workspace]\n# Disable the changelog for all packages.\nchangelog_update = false\nchangelog_path = "./CHANGELOG.md"\n\n[[package]]\nname = "package_a"\n# Enable the changelog for this package (override default).\nchangelog_update = true\n'})})]}),"\n",(0,o.jsxs)(n.admonition,{type:"tip",children:[(0,o.jsxs)(n.p,{children:["You can customize the changelog path for each package.\nIn the following example, the changes of ",(0,o.jsx)(n.code,{children:"package_b"})," will be added to its own changelog,\ninstead of being included in ",(0,o.jsx)(n.code,{children:"./CHANGELOG.md"})," like all the other packages."]}),(0,o.jsx)(n.pre,{children:(0,o.jsx)(n.code,{className:"language-toml",children:'[workspace]\nchangelog_path = "./CHANGELOG.md"\n\n[[package]]\nname = "package_b"\nchangelog_path = "package_b/CHANGELOG.md"\n'})})]})]})}function g(e={}){const{wrapper:n}={...(0,c.R)(),...e.components};return n?(0,o.jsx)(n,{...e,children:(0,o.jsx)(h,{...e})}):h(e)}},8453:(e,n,a)=>{a.d(n,{R:()=>l,x:()=>i});var t=a(6540);const o={},c=t.createContext(o);function l(e){const n=t.useContext(c);return t.useMemo((function(){return"function"==typeof e?e(n):{...n,...e}}),[n,e])}function i(e){let n;return n=e.disableParentContext?"function"==typeof e.components?e.components(o):e.components||o:l(e.components),t.createElement(c.Provider,{value:n},e.children)}}}]);