"use client";

import { useActionState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { addTopicAction, type TopicState } from "./actions";

/** 사용자 주제 넣기(11단계): 적은 그대로 2~20자. 넣으면 켜진 증시 기준으로 트리를 만든다. */
export function TopicForm() {
  const [state, formAction, pending] = useActionState<TopicState, FormData>(
    addTopicAction,
    {},
  );
  return (
    <div>
      {/* 넣을 때마다 key가 바뀌어 입력칸이 비워진다. */}
      <form key={state.added ?? 0} action={formAction} className="flex gap-2">
        <Label htmlFor="topic-name" className="sr-only">
          주제
        </Label>
        <Input
          id="topic-name"
          name="name"
          placeholder="예: 부동산 시장, 중동 정세"
          minLength={2}
          maxLength={20}
          autoComplete="off"
          required
        />
        <Button type="submit" disabled={pending} className="shrink-0">
          {pending ? "넣는 중…" : "주제 추가"}
        </Button>
      </form>
      {state.error && (
        <p role="alert" className="mt-2 text-sm text-destructive">
          {state.error}
        </p>
      )}
    </div>
  );
}
