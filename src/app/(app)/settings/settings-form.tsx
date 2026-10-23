"use client";

import { useActionState, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { saveSettingsAction, type SettingsState } from "./actions";
import type { SettingsFieldName, SettingsValues } from "./fields";

const SELECT_CLASS =
  "h-8 w-full rounded-lg border border-input bg-transparent px-2 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 aria-invalid:border-destructive dark:bg-input/30";

/** 설정 화면 입력(SPEC FR-8). 모든 항목을 "저장" 하나로 저장한다(2026-10-21 결정). */
export function SettingsForm({ initial }: { initial: SettingsValues }) {
  const [state, formAction, pending] = useActionState<SettingsState, FormData>(
    saveSettingsAction,
    { values: initial },
  );
  const value = (field: SettingsFieldName) => state.values[field];
  const error = (field: SettingsFieldName) => state.errors?.[field];

  return (
    <form action={formAction} className="space-y-8" data-testid="settings-form">
      <Section title="수집">
        <Field
          id="collectIntervalMinutes"
          label="수집 간격"
          help="정기 수집 사이 간격. 저장하면 다음 수집부터 적용됩니다."
          error={error("collectIntervalMinutes")}
        >
          <select
            id="collectIntervalMinutes"
            name="collectIntervalMinutes"
            defaultValue={value("collectIntervalMinutes")}
            aria-invalid={Boolean(error("collectIntervalMinutes"))}
            className={SELECT_CLASS}
          >
            <option value="15">15분</option>
            <option value="30">30분</option>
            <option value="60">1시간</option>
            <option value="180">3시간</option>
          </select>
        </Field>
      </Section>

      <Section title="키워드">
        <Field
          id="keywordsDepth"
          label="확장 깊이"
          help="다음 06:00 자동 재생성 때 반영됩니다. 바로 반영하려면 아래 '지금 다시 만들기'를 누르세요."
          error={error("keywordsDepth")}
        >
          <select
            id="keywordsDepth"
            name="keywordsDepth"
            defaultValue={value("keywordsDepth")}
            aria-invalid={Boolean(error("keywordsDepth"))}
            className={SELECT_CLASS}
          >
            {[1, 2, 3, 4].map((depth) => (
              <option key={depth} value={String(depth)}>
                {depth}단계
              </option>
            ))}
          </select>
        </Field>
        <Field
          id="keywordsRegenerateEvery"
          label="자동 재생성 주기"
          error={error("keywordsRegenerateEvery")}
        >
          <select
            id="keywordsRegenerateEvery"
            name="keywordsRegenerateEvery"
            defaultValue={value("keywordsRegenerateEvery")}
            aria-invalid={Boolean(error("keywordsRegenerateEvery"))}
            className={SELECT_CLASS}
          >
            <option value="daily">하루 1회(매일 06:00)</option>
            <option value="weekly">일주일 1회(월요일 06:00)</option>
          </select>
        </Field>
        <ScoreField
          id="keywordMinRelevance"
          label="키워드 남길 관련도(0~10)"
          help="AI가 낸 키워드 후보와 토픽–종목 쌍을 Jev가 매겨, 관련도와 영향도가 둘 다 기준 이상인 것만 남깁니다. 다음 트리 만들기·토픽 갱신부터 적용됩니다."
          value={value("keywordMinRelevance")}
          error={error("keywordMinRelevance")}
        />
        <ScoreField
          id="keywordMinMagnitude"
          label="키워드 남길 영향도(0~10)"
          value={value("keywordMinMagnitude")}
          error={error("keywordMinMagnitude")}
        />
        <Field
          id="llmModel"
          label="키워드·종목 찾기 모델(OpenRouter)"
          help="저장할 때 OpenRouter 모델 목록에 있는지 확인합니다."
          error={error("llmModel")}
        >
          <Input
            id="llmModel"
            name="llmModel"
            defaultValue={value("llmModel")}
            maxLength={200}
            autoComplete="off"
            spellCheck={false}
            aria-invalid={Boolean(error("llmModel"))}
          />
        </Field>
      </Section>

      <Section title="평가">
        <ScoreField
          id="minRelevance"
          label="관련도 표시 기준(0~10)"
          help="얼마나 이 항목 얘기인가. 기준 미만 기사도 지우지 않고 숨기기만 합니다."
          value={value("minRelevance")}
          error={error("minRelevance")}
        />
        <ScoreField
          id="minMagnitude"
          label="영향도 표시 기준(0~10)"
          help="주가·시장을 얼마나 움직일까. 관련도와 영향도가 둘 다 기준 이상인 기사만 보입니다(영향도를 매기기 전 기사는 관련도만 봅니다)."
          value={value("minMagnitude")}
          error={error("minMagnitude")}
        />
        <Field
          id="maxPerRun"
          label="회차당 최대 평가 수"
          help="넘는 기사는 다음 회차로 넘깁니다(종목 이름·직접 연결 먼저, 최근 7일 기사만)."
          error={error("maxPerRun")}
        >
          <Input
            id="maxPerRun"
            name="maxPerRun"
            type="number"
            min={10}
            max={1000}
            step={1}
            inputMode="numeric"
            defaultValue={value("maxPerRun")}
            aria-invalid={Boolean(error("maxPerRun"))}
          />
        </Field>
      </Section>

      <Section title="시장 토픽">
        <Field
          id="topicIntervalHours"
          label="토픽 갱신 간격(시간, 0~24)"
          help="수집한 기사 제목으로 AI가 시장을 움직이는 토픽을 다시 정하는 간격입니다(낮에 1번 약 $0.005). 0이면 간격으로는 갱신하지 않습니다."
          error={error("topicIntervalHours")}
        >
          <Input
            id="topicIntervalHours"
            name="topicIntervalHours"
            type="number"
            min={0}
            max={24}
            step={1}
            inputMode="numeric"
            defaultValue={value("topicIntervalHours")}
            aria-invalid={Boolean(error("topicIntervalHours"))}
          />
        </Field>
        <Field
          id="topicTimes"
          label="토픽 갱신 특정 시각(한국 시간)"
          help="간격과 별개로 이 시각에도 갱신합니다. 쉼표로 구분(예: 08:30, 15:40), 12개까지. 간격 0에 시각도 비우면 토픽 갱신을 끕니다(토픽 카드와 종목에 붙은 토픽이 더 바뀌지 않음)."
          error={error("topicTimes")}
        >
          <Input
            id="topicTimes"
            name="topicTimes"
            defaultValue={value("topicTimes")}
            placeholder="예: 08:30, 15:40"
            maxLength={200}
            autoComplete="off"
            spellCheck={false}
            aria-invalid={Boolean(error("topicTimes"))}
          />
        </Field>
      </Section>

      <Section title="비용">
        <Field
          id="monthlyBudgetUsd"
          label="월 예산 한도(달러)"
          help="이번 달(한국 시간) AI 사용액이 이 값에 닿으면 AI 호출을 멈춥니다. 수집은 계속합니다."
          error={error("monthlyBudgetUsd")}
        >
          <Input
            id="monthlyBudgetUsd"
            name="monthlyBudgetUsd"
            type="number"
            min={0.1}
            max={1000}
            step={0.01}
            inputMode="decimal"
            defaultValue={value("monthlyBudgetUsd")}
            aria-invalid={Boolean(error("monthlyBudgetUsd"))}
          />
        </Field>
      </Section>

      <div className="flex flex-wrap items-center gap-3">
        <Button type="submit" disabled={pending}>
          {pending ? "저장 중…" : "저장"}
        </Button>
        {state.message && (
          <p
            role={state.saved ? "status" : "alert"}
            className={
              state.saved
                ? "text-sm text-muted-foreground"
                : "text-sm text-destructive"
            }
          >
            {state.message}
          </p>
        )}
      </div>
    </form>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <fieldset className="space-y-4">
      <legend className="mb-3 text-base font-semibold">{title}</legend>
      {children}
    </fieldset>
  );
}

/** 0~10 점수 기준 입력칸 */
function ScoreField(props: {
  id: SettingsFieldName;
  label: string;
  help?: string;
  value: string;
  error?: string;
}) {
  return (
    <Field
      id={props.id}
      label={props.label}
      help={props.help}
      error={props.error}
    >
      <Input
        id={props.id}
        name={props.id}
        type="number"
        min={0}
        max={10}
        step={0.5}
        inputMode="decimal"
        defaultValue={props.value}
        aria-invalid={Boolean(props.error)}
      />
    </Field>
  );
}

function Field(props: {
  id: string;
  label: string;
  help?: string;
  error?: string;
  children: ReactNode;
}) {
  return (
    <div className="space-y-1.5">
      <Label htmlFor={props.id}>{props.label}</Label>
      {props.children}
      {props.error ? (
        <p
          className="text-xs text-destructive"
          data-testid={`${props.id}-error`}
        >
          {props.error}
        </p>
      ) : (
        props.help && (
          <p className="text-xs text-muted-foreground">{props.help}</p>
        )
      )}
    </div>
  );
}
