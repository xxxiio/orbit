import { Skeleton } from '@/components/ui/skeleton'

export function DrawerSkeleton() {
  return (
    <div className="flex h-full flex-col" aria-busy="true" aria-label="Loading task">
      <div className="border-b border-line px-5 pb-4 pr-12 pt-5">
        <Skeleton className="h-2.5 w-24" />
        <Skeleton className="mt-3 h-6 w-4/5" />
        <Skeleton className="mt-2 h-2.5 w-32" />
      </div>
      <div className="flex-1 space-y-6 px-5 py-5">
        <div className="grid gap-3 sm:grid-cols-2">
          {Array.from({ length: 4 }, (_, index) => (
            <div key={index} className="space-y-2">
              <Skeleton className="h-2 w-16" />
              <Skeleton className="h-8 w-full" />
            </div>
          ))}
        </div>
        <div className="space-y-2">
          <Skeleton className="h-2 w-20" />
          <Skeleton className="h-24 w-full" />
        </div>
        <div className="space-y-2">
          <Skeleton className="h-2 w-28" />
          <Skeleton className="h-3 w-full" />
          <Skeleton className="h-3 w-5/6" />
          <Skeleton className="h-3 w-2/3" />
        </div>
      </div>
    </div>
  )
}
