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
import { deleteTopicAction, type DeleteState } from "./actions";

/** 주제 삭제 전 확인창(11단계). 항목은 숨기고 모은 기사와 판정은 보관한다. */
export function DeleteTopicButton(props: { id: number; name: string }) {
  const [state, formAction, pending] = useActionState<DeleteState, FormData>(
    deleteTopicAction,
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
          <AlertDialogTitle>{props.name} 주제를 삭제할까요?</AlertDialogTitle>
          <AlertDialogDescription>
            이 주제로는 기사를 더 모으지 않습니다. 이미 모은 기사와 판정은
            보관됩니다.
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
