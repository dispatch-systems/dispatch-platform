import fs from 'node:fs';
import path from 'node:path';

/**
 * The assessment fixture the tools job of this ref or of main built from identical Rust
 * inputs and CI restored into this checkout, or undefined when the debug example must be
 * built here. Only the workspace's own `.ci-tools` directory is trusted, and only on CI;
 * the same rule as `tooling/cli/dispatchdev` applies to dispatchdev itself.
 */
export function assessmentFixture(
  env: Record<string, string | undefined>,
  root: string,
): string | undefined {
  const tools = env.DISPATCH_CI_TOOLS;
  if (env.CI !== 'true' || !tools || path.resolve(tools) !== path.resolve(root, '.ci-tools'))
    return undefined;
  return executable(path.join(root, '.ci-tools/fixture/assessment-fixture'));
}

/**
 * Off CI, the assessment fixture DISPATCH_ASSESSMENT_FIXTURE names: `dispatchdev test` names the
 * one it built or reused for this checkout's Rust inputs. Undefined when it must be built here.
 */
export function namedFixture(env: Record<string, string | undefined>): string | undefined {
  const binary = env.DISPATCH_ASSESSMENT_FIXTURE;
  return env.CI === 'true' || !binary ? undefined : executable(binary);
}

/** `binary` when it is an executable regular file, not a link to one. */
function executable(binary: string): string | undefined {
  let info: fs.Stats;
  try {
    info = fs.lstatSync(binary);
  } catch {
    return undefined;
  }
  if (!info.isFile()) return undefined;
  try {
    fs.accessSync(binary, fs.constants.X_OK);
  } catch {
    return undefined;
  }
  return binary;
}
