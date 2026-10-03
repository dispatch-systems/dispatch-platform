import { Component, type ReactNode } from 'react';

/** A failed route download leaves a recoverable screen instead of a blank dashboard. */
export class PageBoundary extends Component<
  { children: ReactNode; resetKey?: string },
  { failed: boolean }
> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  componentDidUpdate(previous: Readonly<{ children: ReactNode; resetKey?: string }>) {
    if (this.state.failed && previous.resetKey !== this.props.resetKey)
      this.setState({ failed: false });
  }
  render() {
    return this.state.failed ? (
      <section role="alert">
        <p>This page could not load. Check your connection and try again.</p>
        <button onClick={() => location.reload()}>Reload page</button>
      </section>
    ) : (
      this.props.children
    );
  }
}
