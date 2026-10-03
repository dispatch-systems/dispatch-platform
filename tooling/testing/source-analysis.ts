import fs from 'node:fs';
import path from 'node:path';
import ts from 'typescript';

/** Module references in TypeScript/JavaScript, including type-only and lazy imports. */
export function moduleSpecifiers(text: string, file: string): string[] {
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  const references = new Set<string>();
  const add = (node: ts.Node | undefined) => {
    if (node && ts.isStringLiteralLike(node)) references.add(node.text);
  };
  const visit = (node: ts.Node) => {
    if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) add(node.moduleSpecifier);
    else if (
      ts.isImportEqualsDeclaration(node) &&
      ts.isExternalModuleReference(node.moduleReference)
    )
      add(node.moduleReference.expression);
    else if (ts.isImportTypeNode(node) && ts.isLiteralTypeNode(node.argument))
      add(node.argument.literal);
    else if (
      ts.isCallExpression(node) &&
      (node.expression.kind === ts.SyntaxKind.ImportKeyword ||
        (ts.isIdentifier(node.expression) && node.expression.text === 'require'))
    )
      add(node.arguments[0]);
    ts.forEachChild(node, visit);
  };
  visit(source);
  return [...references];
}

/** The modules a file loads with it: value imports and re-exports, not lazy or type-only ones. */
export function eagerSpecifiers(text: string, file: string): string[] {
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  return source.statements.flatMap((statement) =>
    (ts.isImportDeclaration(statement) && !statement.importClause?.isTypeOnly) ||
    (ts.isExportDeclaration(statement) && !statement.isTypeOnly)
      ? [statement.moduleSpecifier]
          .filter((node) => node && ts.isStringLiteralLike(node))
          .map((node) => (node as ts.StringLiteralLike).text)
      : [],
  );
}

/** The modules a file loads lazily, with `import()`. */
export function lazySpecifiers(text: string, file: string): string[] {
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  const references: string[] = [];
  const visit = (node: ts.Node) => {
    if (
      ts.isCallExpression(node) &&
      node.expression.kind === ts.SyntaxKind.ImportKeyword &&
      node.arguments[0] &&
      ts.isStringLiteralLike(node.arguments[0])
    )
      references.push(node.arguments[0].text);
    ts.forEachChild(node, visit);
  };
  visit(source);
  return references;
}

/** Use the compiler's extension substitution and directory-index resolution. */
export function resolveModule(file: string, specifier: string): string | undefined {
  const resolved = ts.resolveModuleName(
    specifier.split('?')[0]!,
    path.resolve(file),
    {
      module: ts.ModuleKind.ESNext,
      moduleResolution: ts.ModuleResolutionKind.Bundler,
      allowJs: true,
      resolveJsonModule: true,
    },
    ts.sys,
  ).resolvedModule;
  return resolved && path.resolve(resolved.resolvedFileName);
}

/** Includes styles/artwork references that TypeScript does not resolve as modules. */
export function dependencies(file: string) {
  return moduleSpecifiers(fs.readFileSync(file, 'utf8'), file).map((specifier) => ({
    specifier,
    resolved: resolveModule(file, specifier),
    target: specifier.startsWith('.')
      ? path.resolve(path.dirname(file), specifier.split('?')[0]!)
      : undefined,
  }));
}

/** Environment reads are syntax, not text in comments or fixture strings. */
export function readsEnvironment(text: string, file: string, variable: string): boolean {
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  let found = false;
  const visit = (node: ts.Node) => {
    if (
      ts.isPropertyAccessExpression(node) &&
      node.name.text === variable &&
      ts.isPropertyAccessExpression(node.expression) &&
      node.expression.name.text === 'env' &&
      ts.isIdentifier(node.expression.expression) &&
      node.expression.expression.text === 'process'
    )
      found = true;
    if (
      ts.isElementAccessExpression(node) &&
      ts.isStringLiteralLike(node.argumentExpression) &&
      node.argumentExpression.text === variable &&
      ts.isPropertyAccessExpression(node.expression) &&
      node.expression.name.text === 'env' &&
      ts.isIdentifier(node.expression.expression) &&
      node.expression.expression.text === 'process'
    )
      found = true;
    ts.forEachChild(node, visit);
  };
  visit(source);
  return found;
}
