"use client";

import { useNavigationGuard } from "nextjs-nav-guard";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";

export function DirtyFormGuard({ dirty }: Readonly<{ dirty: boolean }>) {
  const guard = useNavigationGuard({ enabled: dirty });

  return (
    <AlertDialog
      open={guard.active}
      onOpenChange={(open) => {
        if (!open) {
          guard.reject();
        }
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Discard unsaved changes?</AlertDialogTitle>
          <AlertDialogDescription>
            Your changes have not been saved.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel onClick={guard.reject}>
            Keep editing
          </AlertDialogCancel>
          <AlertDialogAction variant="destructive" onClick={guard.accept}>
            Discard changes
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
