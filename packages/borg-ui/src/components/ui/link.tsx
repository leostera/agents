import * as React from 'react'

type LinkProps = React.ComponentProps<'a'> & {
  href: string
}

function shouldHandleSpaNavigation(event: React.MouseEvent<HTMLAnchorElement>, href: string): boolean {
  if (event.defaultPrevented) return false
  if (event.button !== 0) return false
  if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return false
  if (!href || href.startsWith('http://') || href.startsWith('https://')) return false
  return true
}

export const Link = React.forwardRef<HTMLAnchorElement, LinkProps>(
  ({ href, onClick, target, rel, ...props }, ref) => {
    return (
      <a
        ref={ref}
        href={href}
        target={target}
        rel={rel}
        onClick={(event) => {
          onClick?.(event)
          if (!shouldHandleSpaNavigation(event, href)) return
          if (target && target !== '_self') return
          if (typeof window === 'undefined') return

          event.preventDefault()
          window.history.pushState(null, '', href)
          window.dispatchEvent(new PopStateEvent('popstate'))
        }}
        {...props}
      />
    )
  }
)

Link.displayName = 'Link'

