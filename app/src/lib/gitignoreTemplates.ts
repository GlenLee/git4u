export interface GitignoreTemplate {
  id: string;
  name: string;
  /** 提示文案的 i18n key。 */
  hint: string;
  content: string;
}

/** 常用语言、编辑器与操作系统的起步模板；用户可多选后追加到 .gitignore。 */
export const GITIGNORE_TEMPLATES: GitignoreTemplate[] = [
  {
    id: "node",
    name: "Node.js",
    hint: "gitignore.node",
    content: `node_modules/
dist/
build/
coverage/
.env
.env.*
!.env.example
*.log
npm-debug.log*
pnpm-debug.log*
`,
  },
  {
    id: "rust",
    name: "Rust",
    hint: "gitignore.rust",
    content: `target/
**/*.rs.bk
*.pdb
`,
  },
  {
    id: "python",
    name: "Python",
    hint: "gitignore.python",
    content: `__pycache__/
*.py[cod]
*.egg-info/
.venv/
venv/
.pytest_cache/
.mypy_cache/
.ruff_cache/
dist/
build/
`,
  },
  {
    id: "java",
    name: "Java",
    hint: "gitignore.java",
    content: `target/
build/
out/
.gradle/
*.class
*.log
`,
  },
  {
    id: "go",
    name: "Go",
    hint: "gitignore.go",
    content: `bin/
*.exe
*.test
*.out
vendor/
`,
  },
  {
    id: "cpp",
    name: "C / C++",
    hint: "gitignore.cpp",
    content: `build/
out/
*.o
*.obj
*.a
*.so
*.dylib
*.dll
*.exe
`,
  },
  {
    id: "jetbrains",
    name: "JetBrains",
    hint: "gitignore.jetbrains",
    content: `.idea/
*.iml
*.ipr
*.iws
out/
`,
  },
  {
    id: "vscode",
    name: "VS Code",
    hint: "gitignore.vscode",
    content: `.vscode/
*.code-workspace
`,
  },
  {
    id: "macos",
    name: "macOS",
    hint: "gitignore.macos",
    content: `.DS_Store
.AppleDouble
.LSOverride
._*
`,
  },
  {
    id: "windows",
    name: "Windows",
    hint: "gitignore.windows",
    content: `Thumbs.db
ehthumbs.db
Desktop.ini
$RECYCLE.BIN/
`,
  },
  {
    id: "linux",
    name: "Linux",
    hint: "gitignore.linux",
    content: `*~
.directory
`,
  },
  {
    id: "unity",
    name: "Unity",
    hint: "gitignore.unity",
    content: `[Ll]ibrary/
[Tt]emp/
[Oo]bj/
[Bb]uild/
[Bb]uilds/
[Ll]ogs/
[Uu]ser[Ss]ettings/
`,
  },
];

export function gitignoreBlock(template: GitignoreTemplate): string {
  return `# ${template.name}\n${template.content.trim()}\n`;
}
