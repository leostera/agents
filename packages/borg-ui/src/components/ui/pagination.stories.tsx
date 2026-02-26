import type { Meta, StoryObj } from '@storybook/react'
import { useMemo, useState } from 'react'

import {
  Pagination,
  PaginationContent,
  PaginationEllipsis,
  PaginationItem,
  PaginationLink,
  PaginationNext,
  PaginationPrevious,
} from './pagination'

const meta: Meta<typeof Pagination> = {
  title: 'UI/Pagination',
  component: Pagination,
}

export default meta
type Story = StoryObj<typeof Pagination>

export const Default: Story = {
  render: () => {
    const totalPages = 9
    const [page, setPage] = useState(4)

    const visiblePages = useMemo(() => {
      if (page <= 2) return [1, 2, 3]
      if (page >= totalPages - 1) return [totalPages - 2, totalPages - 1, totalPages]
      return [page - 1, page, page + 1]
    }, [page, totalPages])

    return (
      <Pagination>
        <PaginationContent>
          <PaginationItem>
            <PaginationPrevious
              href='#'
              onClick={(event) => {
                event.preventDefault()
                setPage((current) => Math.max(1, current - 1))
              }}
            />
          </PaginationItem>

          <PaginationItem>
            <PaginationLink
              href='#'
              isActive={page === 1}
              onClick={(event) => {
                event.preventDefault()
                setPage(1)
              }}
            >
              1
            </PaginationLink>
          </PaginationItem>

          {visiblePages[0] > 2 && (
            <PaginationItem>
              <PaginationEllipsis />
            </PaginationItem>
          )}

          {visiblePages
            .filter((value) => value !== 1 && value !== totalPages)
            .map((value) => (
              <PaginationItem key={value}>
                <PaginationLink
                  href='#'
                  isActive={page === value}
                  onClick={(event) => {
                    event.preventDefault()
                    setPage(value)
                  }}
                >
                  {value}
                </PaginationLink>
              </PaginationItem>
            ))}

          {visiblePages[visiblePages.length - 1] < totalPages - 1 && (
            <PaginationItem>
              <PaginationEllipsis />
            </PaginationItem>
          )}

          <PaginationItem>
            <PaginationLink
              href='#'
              isActive={page === totalPages}
              onClick={(event) => {
                event.preventDefault()
                setPage(totalPages)
              }}
            >
              {totalPages}
            </PaginationLink>
          </PaginationItem>

          <PaginationItem>
            <PaginationNext
              href='#'
              onClick={(event) => {
                event.preventDefault()
                setPage((current) => Math.min(totalPages, current + 1))
              }}
            />
          </PaginationItem>
        </PaginationContent>
      </Pagination>
    )
  },
}
