// The TypeScript linter: what the compiler's strictness leaves to a linter, as hooks called out
// of order or with dependencies left out, promises left floating, and values thrown that
// aren't errors. `npm run lint` runs it over every TypeScript file; `dispatchdev check` runs it
// before a push. Prettier keeps the formatting, so no rule here is about layout.
//
// The product's code holds the rules that need the types, as a promise left floating or a
// value read off `any`. Tests and tooling read loosely typed JSON on purpose, so they hold the
// plain rules alone.
import js from '@eslint/js';
import reactHooks from 'eslint-plugin-react-hooks';
import globals from 'globals';
import tseslint from 'typescript-eslint';

const tests = ['**/tests/**', '**/test/**', 'tooling/**', 'services/**', 'ops/**'];

export default tseslint.config(
  { ignores: ['**/node_modules/', '**/generated/', '**/dist/', 'target/'] },
  {
    files: ['**/*.ts', '**/*.tsx'],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: { globals: { ...globals.browser, ...globals.node } },
    rules: {
      // A name beginning with `_` is set aside on purpose.
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_', caughtErrorsIgnorePattern: '^_' },
      ],
      // An empty interface is how an owner's pages join a slot's, by declaration merging.
      '@typescript-eslint/no-empty-object-type': ['error', { allowInterfaces: 'always' }],
      // An empty catch says the failure is nothing to act on.
      'no-empty': ['error', { allowEmptyCatch: true }],
    },
  },
  {
    files: ['**/*.ts', '**/*.tsx'],
    ignores: tests,
    extends: [...tseslint.configs.recommendedTypeChecked],
    languageOptions: {
      parserOptions: { projectService: true, tsconfigRootDir: import.meta.dirname },
    },
    rules: {
      // An async handler on an element is how React takes one; the promise is the handler's.
      '@typescript-eslint/no-misused-promises': [
        'error',
        { checksVoidReturn: { attributes: false } },
      ],
    },
  },
  {
    // A test reads what the product answers as it is, and a Playwright fixture takes what it
    // doesn't use as an empty pattern.
    files: tests,
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
      'no-empty-pattern': ['error', { allowObjectPatternsAsParameters: true }],
    },
  },
  {
    // React runs in the frontends alone, so its hooks' rules hold there and nowhere else.
    files: ['**/frontend/**/*.ts', '**/frontend/**/*.tsx'],
    extends: [reactHooks.configs.flat['recommended-latest']],
    rules: {
      // The React Compiler's rules, which the code doesn't run under, each flagging a pattern
      // the code uses on purpose: setting state in an effect that follows a read; keeping a ref
      // current during render, as `useDataTable` keeps the latest columns for its callbacks and
      // the onboarding layouts keep their steps; and reading the clock during render, as the
      // audit log names today and a dialog counts down. Under the compiler each would need its
      // own shape; without it they are React as written.
      'react-hooks/set-state-in-effect': 'off',
      'react-hooks/refs': 'off',
      'react-hooks/purity': 'off',
    },
  },
);
