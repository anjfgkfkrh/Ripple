"use client";

import { useActionState } from "react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { deleteStockAction, type DeleteState } from "./actions";

/** 삭제 전 확인창. 지우지 않고 숨기므로 다시 등록하면 되살아난다(SPEC D-11). */
export function DeleteStockButton(props: { id: number; name: string }) {
  const [state, formAction, pending] = useActionState<DeleteState, FormData>(
    deleteStockAction,
    {},
  );

  return (
    <AlertDialog>
      <AlertDialogTrigger
        render={
          <Button
            variant="ghost"
            size="sm"
            className="shrink-0"
            aria-label={`${props.name} 삭제`}
          />
        }
      >
        삭제
      </AlertDialogTrigger>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{props.name} 종목을 삭제할까요?</AlertDialogTitle>
          <AlertDialogDescription>
            수집 대상에서 빠집니다. 이미 모은 기사와 판정은 보관되고, 다시
            등록하면 되살아납니다.
          </AlertDialogDescription>
        </AlertDialogHeader>
        {state.error && (
          <p role="alert" className="text-sm text-destructive">
            {state.error}
          </p>
        )}
        <AlertDialogFooter>
          <AlertDialogCancel>취소</AlertDialogCancel>
          <form action={formAction}>
            <input type="hidden" name="id" value={props.id} />
            <AlertDialogAction
              type="submit"
              variant="destructive"
              disabled={pending}
              className="w-full"
            >
              {pending ? "삭제 중…" : "삭제"}
            </AlertDialogAction>
          </form>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
