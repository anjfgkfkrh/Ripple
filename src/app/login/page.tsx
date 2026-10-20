import { redirect } from "next/navigation";
import { Suspense } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { hasValidSession } from "@/server/auth/guard";
import { safeNextPath } from "@/server/auth/request";
import { LoginForm } from "./login-form";

export default function LoginPage({ searchParams }: PageProps<"/login">) {
  return (
    <main className="flex flex-1 items-center justify-center px-4 py-12">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle>
            <h1>Ripple</h1>
          </CardTitle>
          <CardDescription>비밀번호를 입력하세요.</CardDescription>
        </CardHeader>
        <CardContent>
          <Suspense>
            <LoginFormSection searchParams={searchParams} />
          </Suspense>
        </CardContent>
      </Card>
    </main>
  );
}

async function LoginFormSection({
  searchParams,
}: Pick<PageProps<"/login">, "searchParams">) {
  const next = safeNextPath((await searchParams).next);
  // 이미 로그인했으면 가려던 곳으로 보낸다.
  if (await hasValidSession()) redirect(next);
  return <LoginForm next={next} />;
}
