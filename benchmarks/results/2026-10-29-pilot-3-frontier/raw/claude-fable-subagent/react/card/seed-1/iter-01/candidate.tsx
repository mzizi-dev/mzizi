// CARD — N2 primitive, React.
// A container for grouped content, made of seven parts: card, header, title,
// description, action, content and footer. The card renders a skeleton when loading.
import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const cardVariants = cva(
  "group/card flex flex-col gap-6 overflow-hidden rounded-[var(--radius-lg,14px)] bg-card py-6 text-sm text-card-foreground ring-1 ring-foreground/10 has-[>img:first-child]:pt-0 data-[size=sm]:gap-4 data-[size=sm]:py-4 *:[img:first-child]:rounded-t-[var(--radius-md,12px)] *:[img:last-child]:rounded-b-[var(--radius-md,12px)]",
  {
    variants: {
      size: {
        default: "",
        sm: "",
      },
    },
    defaultVariants: {
      size: "default",
    },
  }
)

const cardSkeletonVariants = cva(
  "group/card flex animate-pulse flex-col gap-4 overflow-hidden rounded-[var(--radius-lg,14px)] bg-card p-6 text-sm ring-1 ring-foreground/10",
  {
    variants: {
      size: {
        default: "",
        sm: "gap-3 p-4",
      },
    },
    defaultVariants: {
      size: "default",
    },
  }
)

type CardProps = React.ComponentProps<"div"> &
  VariantProps<typeof cardVariants> & {
    loading?: boolean
  }

function Card({ className, size = "default", loading = false, children, ...props }: CardProps) {
  if (loading) {
    return (
      <div
        data-slot="card"
        data-portal="https://mzizi.dev/components/card"
        data-size={size}
        data-loading=""
        className={cn(cardSkeletonVariants({ size }), className)}
        {...props}
      >
        <div className="h-4 w-2/3 rounded bg-muted" />
        <div className="h-3 w-full rounded bg-muted" />
        <div className="h-3 w-4/5 rounded bg-muted" />
      </div>
    )
  }

  return (
    <div
      data-slot="card"
      data-portal="https://mzizi.dev/components/card"
      data-size={size}
      className={cn(cardVariants({ size }), className)}
      {...props}
    >
      {children}
    </div>
  )
}

type CardHeaderProps = React.ComponentProps<"div">

function CardHeader({ className, children, ...props }: CardHeaderProps) {
  return (
    <div
      data-slot="card-header"
      className={cn(
        "group/card-header @container/card-header grid auto-rows-min items-start gap-2 rounded-t-[var(--radius-md,12px)] px-6 group-data-[size=sm]/card:px-4 has-data-[slot=card-action]:grid-cols-[1fr_auto] has-data-[slot=card-description]:grid-rows-[auto_auto] [.border-b]:pb-6 group-data-[size=sm]/card:[.border-b]:pb-4",
        className
      )}
      {...props}
    >
      {children}
    </div>
  )
}

type CardTitleProps = React.ComponentProps<"div">

function CardTitle({ className, children, ...props }: CardTitleProps) {
  return (
    <div data-slot="card-title" className={cn("text-base font-medium", className)} {...props}>
      {children}
    </div>
  )
}

type CardDescriptionProps = React.ComponentProps<"div">

function CardDescription({ className, children, ...props }: CardDescriptionProps) {
  return (
    <div
      data-slot="card-description"
      className={cn("text-sm text-muted-foreground", className)}
      {...props}
    >
      {children}
    </div>
  )
}

type CardActionProps = React.ComponentProps<"div">

function CardAction({ className, children, ...props }: CardActionProps) {
  return (
    <div
      data-slot="card-action"
      className={cn("col-start-2 row-span-2 row-start-1 self-start justify-self-end", className)}
      {...props}
    >
      {children}
    </div>
  )
}

type CardContentProps = React.ComponentProps<"div">

function CardContent({ className, children, ...props }: CardContentProps) {
  return (
    <div
      data-slot="card-content"
      className={cn("px-6 group-data-[size=sm]/card:px-4", className)}
      {...props}
    >
      {children}
    </div>
  )
}

type CardFooterProps = React.ComponentProps<"div">

function CardFooter({ className, children, ...props }: CardFooterProps) {
  return (
    <div
      data-slot="card-footer"
      className={cn(
        "flex items-center rounded-b-[var(--radius-md,12px)] px-6 group-data-[size=sm]/card:px-4 [.border-t]:pt-6 group-data-[size=sm]/card:[.border-t]:pt-4",
        className
      )}
      {...props}
    >
      {children}
    </div>
  )
}

export {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardAction,
  CardContent,
  CardFooter,
  cardVariants,
}
