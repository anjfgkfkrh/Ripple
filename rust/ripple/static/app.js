// 작은 화면 동작(설계 5.4). 프레임워크 없이 이벤트 위임으로 쓴다.
(() => {
  // 제출 중 단추를 잠그고 글자를 바꾼다. 비밀번호 칸은 잠그지 않는다(잠긴 칸은 제출되지 않는다).
  function lockButton(form) {
    const label = form.dataset.pendingLabel;
    const button = form.querySelector("button[type=submit]");
    if (!label || !button || button.disabled) return;
    button.dataset.idleLabel = button.textContent;
    button.disabled = true;
    button.textContent = label;
  }

  // 요청이 실패해도 응답이 바꿔 끼워지지 않으면 단추가 새로 그려지지 않으므로 직접 푼다.
  function unlockButton(form) {
    const button = form.querySelector("button[type=submit]");
    if (!button || !button.disabled) return;
    button.disabled = false;
    if (button.dataset.idleLabel) button.textContent = button.dataset.idleLabel;
  }

  // 위 메뉴에서 지금 있는 곳을 굵게 표시한다. 타임라인("/")은 정확히 같을 때만, 나머지는 하위 경로까지 맞춘다.
  function markCurrentMenu() {
    const path = location.pathname;
    document.querySelectorAll('nav[aria-label="메뉴"] a').forEach((link) => {
      const href = link.getAttribute("href");
      const current =
        href === "/"
          ? path === "/"
          : path === href || path.startsWith(href + "/");
      link.classList.toggle("font-semibold", current);
      link.classList.toggle("text-foreground", current);
      link.classList.toggle("text-muted-foreground", !current);
      if (current) link.setAttribute("aria-current", "page");
      else link.removeAttribute("aria-current");
    });
  }
  markCurrentMenu();
  document.addEventListener("htmx:pushedIntoHistory", markCurrentMenu);
  window.addEventListener("popstate", markCurrentMenu);

  // 일반 폼(로그인)
  document.addEventListener("submit", (event) => {
    const form = event.target;
    if (!(form instanceof HTMLFormElement) || form.hasAttribute("hx-post"))
      return;
    lockButton(form);
  });

  // htmx 폼(지금 수집 등): 응답으로 패널이 통째로 바뀌므로 단추는 따로 되돌리지 않는다.
  document.addEventListener("htmx:beforeRequest", (event) => {
    const form = event.detail && event.detail.elt;
    if (form instanceof HTMLFormElement) lockButton(form);
  });

  // 뒤로 가기로 돌아왔을 때 잠긴 단추가 남지 않게 한다.
  window.addEventListener("pageshow", (event) => {
    if (!event.persisted) return;
    document
      .querySelectorAll("form[data-pending-label] button[type=submit]")
      .forEach((button) => {
        button.disabled = false;
      });
  });

  // 읽음 표시(12단계, 설계 5.4). 카드 안 원문 링크(가운데 버튼 포함)를 누르면 카드에 묶인 기사를 모두 읽음으로
  // 저장하고, 카드 아래 작은 버튼으로 읽음/안 읽음을 되돌린다. 화면은 먼저 바꾸고 저장하며,
  // 실패하면 원래대로 돌리고 이유를 보인다. 읽은 카드의 흐린 글자는 마크업의 `group-data-[read=true]:`가 정한다.
  const CARD = 'article[data-testid="timeline-card"]';
  const READ_FAILED = "읽음 표시를 저장하지 못했습니다.";

  function showRead(article, read) {
    article.dataset.read = String(read);
    article.classList.toggle("bg-muted/40", read);
    const footer = article.querySelector("[data-read-toggle]").parentElement;
    const label = footer.querySelector('[data-testid="read-label"]');
    if (read && !label) {
      const span = document.createElement("span");
      span.dataset.testid = "read-label";
      span.textContent = "읽음 ·";
      footer.insertBefore(span, footer.querySelector("[data-read-toggle]"));
    } else if (!read && label) {
      label.remove();
    }
    const toggle = footer.querySelector("[data-read-toggle]");
    toggle.textContent = read ? "안 읽음으로" : "읽음으로 표시";
    toggle.setAttribute(
      "aria-label",
      `${article.dataset.title} — ${read ? "안 읽음으로" : "읽음으로 표시"}`,
    );
  }

  function showReadError(article, message) {
    const footer = article.querySelector("[data-read-toggle]").parentElement;
    let alert = footer.querySelector('[role="alert"]');
    if (!message) {
      if (alert) alert.remove();
      return;
    }
    if (!alert) {
      alert = document.createElement("span");
      alert.setAttribute("role", "alert");
      alert.className = "mr-auto text-destructive";
      footer.insertBefore(alert, footer.firstChild);
    }
    alert.textContent = message;
  }

  // 손님의 읽음은 이 브라우저에만 저장한다(서버 읽음은 건드리지 않는다). 기사 id 배열, 최근 3000개.
  const GUEST_READS_KEY = "ripple:reads";
  const GUEST_READS_MAX = 3000;
  const isGuest = () => document.body.dataset.access === "guest";

  function guestReadIds() {
    try {
      const ids = JSON.parse(localStorage.getItem(GUEST_READS_KEY) || "[]");
      return Array.isArray(ids) ? ids.filter(Number.isFinite) : [];
    } catch {
      return [];
    }
  }

  function saveGuestRead(articleIds, read) {
    try {
      const rest = guestReadIds().filter((id) => !articleIds.includes(id));
      const next = read ? rest.concat(articleIds) : rest;
      localStorage.setItem(
        GUEST_READS_KEY,
        JSON.stringify(next.slice(-GUEST_READS_MAX)),
      );
    } catch {
      // 저장소를 못 쓰면 화면만 바꾼다.
    }
  }

  // 서버 카드 읽음 규칙과 같게, 카드의 기사 중 하나라도 읽었으면 읽음으로 보인다.
  function applyGuestReads() {
    if (!isGuest()) return;
    const saved = new Set(guestReadIds());
    if (saved.size === 0) return;
    document.querySelectorAll(CARD).forEach((article) => {
      if (article.dataset.read === "true") return;
      const ids = article.dataset.articleIds.split(",").map(Number);
      if (ids.some((id) => saved.has(id))) showRead(article, true);
    });
  }
  applyGuestReads();
  document.addEventListener("htmx:afterSettle", applyGuestReads);

  async function saveRead(article, read) {
    if (isGuest()) {
      const ids = article.dataset.articleIds.split(",").map(Number);
      showReadError(article, null);
      showRead(article, read);
      saveGuestRead(ids, read);
      return;
    }
    const before = article.dataset.read === "true";
    const articleIds = article.dataset.articleIds.split(",").map(Number);
    showReadError(article, null);
    showRead(article, read);
    let message = null;
    try {
      const response = await fetch("/articles/read", {
        method: "POST",
        credentials: "same-origin",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ articleIds, read }),
      });
      const data = await response.json().catch(() => ({}));
      if (!response.ok || data.ok !== true) message = data.error || READ_FAILED;
    } catch {
      message = READ_FAILED;
    }
    if (message) {
      showRead(article, before);
      showReadError(article, message);
    }
  }

  function openedLink(event) {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const article = target.closest(CARD);
    if (!article || article.dataset.read === "true") return;
    if (target.closest("a[href]")) saveRead(article, true);
  }

  document.addEventListener("click", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const toggle = target.closest("[data-read-toggle]");
    const article = toggle && toggle.closest(CARD);
    if (article) {
      saveRead(article, article.dataset.read !== "true");
      return;
    }
    openedLink(event);
  });

  document.addEventListener("auxclick", (event) => {
    if (event.button === 1) openedLink(event);
  });

  // 확인창(14단계 R12): 삭제 단추는 네이티브 dialog를 열고, 취소 단추로 닫는다(Esc는 브라우저가 닫는다).
  document.addEventListener("click", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const opener = target.closest("[data-dialog-open]");
    if (opener) {
      document.getElementById(opener.dataset.dialogOpen).showModal();
      return;
    }
    const closer = target.closest("[data-dialog-close]");
    if (closer) closer.closest("dialog").close();
  });

  // 목록 조각을 다시 그려도 펼쳐 둔 키워드 트리는 펼친 채로 둔다(Next는 React가 펼침 상태를 그대로 뒀다).
  let openDetails = null;
  document.addEventListener("htmx:beforeSwap", () => {
    openDetails = [...document.querySelectorAll("details[id][open]")].map(
      (el) => el.id,
    );
  });
  document.addEventListener("htmx:afterSettle", () => {
    if (!openDetails) return;
    openDetails.forEach((id) => {
      const el = document.getElementById(id);
      if (el) el.open = true;
    });
    openDetails = null;
  });

  // 만드는 중인 목록을 3초마다 다시 읽는 조각(data-poll)은 확인창이 열려 있으면 건너뛴다. 조각을 다시 그리면
  // 열린 확인창도 사라지기 때문이다(Next는 React 상태를 그대로 둬서 창이 열린 채 남았다).
  document.addEventListener("htmx:beforeRequest", (event) => {
    const elt = event.detail && event.detail.elt;
    if (
      elt &&
      elt.hasAttribute("data-poll") &&
      document.querySelector("dialog[open]")
    ) {
      event.preventDefault();
    }
  });

  // 수정 권한이 없을 때(손님이거나 시간이 지남) 서버가 403과 `HX-Trigger: unlock-required`로 답한다.
  // 비밀번호 창을 열고, 맞으면 그 요청을 한 번 다시 보낸다. 비밀번호 검사와 권한 판단은 서버만 한다.
  const unlockDialog = document.getElementById("unlock-dialog");
  const unlockForm = document.getElementById("unlock-form");
  const unlockInput = document.getElementById("unlock-password");
  const unlockError = document.getElementById("unlock-error");
  const NO_PERMISSION = "수정 권한을 얻지 못했습니다.";
  let pendingElt = null; // 비밀번호를 기다리는 원래 요소
  let retryElt = null; // 비밀번호 뒤 다시 보낸 요소(또 403이면 창을 다시 열지 않는다)

  function showUnlockError(message) {
    unlockError.textContent = message || "";
    unlockError.classList.toggle("hidden", !message);
  }

  function failureNote(elt) {
    return (
      elt.parentElement &&
      elt.parentElement.querySelector("[data-unlock-failed]")
    );
  }

  document.addEventListener("htmx:beforeRequest", (event) => {
    const elt = event.detail && event.detail.elt;
    const note = elt && failureNote(elt);
    if (note) note.remove();
  });

  document.addEventListener("htmx:afterRequest", (event) => {
    const elt = event.detail && event.detail.elt;
    if (elt && elt === retryElt) retryElt = null;
  });

  document.addEventListener("htmx:responseError", (event) => {
    const { elt, xhr } = event.detail;
    if (!xhr || xhr.status !== 403) return;
    const trigger = xhr.getResponseHeader("HX-Trigger") || "";
    if (!trigger.includes("unlock-required") || !unlockDialog) return;
    if (elt instanceof HTMLFormElement) unlockButton(elt);
    if (elt === retryElt) {
      retryElt = null;
      if (!failureNote(elt)) {
        const note = document.createElement("p");
        note.dataset.unlockFailed = "";
        note.setAttribute("role", "alert");
        note.className = "text-sm text-destructive";
        note.textContent = NO_PERMISSION;
        elt.insertAdjacentElement("afterend", note);
      }
      return;
    }
    pendingElt = elt;
    unlockInput.value = "";
    showUnlockError(null);
    if (!unlockDialog.open) unlockDialog.showModal();
  });

  if (unlockDialog && unlockForm) wireUnlockForm();

  function wireUnlockForm() {
    unlockDialog.addEventListener("close", () => {
      pendingElt = null;
    });

    unlockForm.addEventListener("submit", async (event) => {
      event.preventDefault();
      const submit = unlockForm.querySelector("button[type=submit]");
      submit.disabled = true;
      try {
        const response = await fetch("/unlock", {
          method: "POST",
          credentials: "same-origin",
          body: new URLSearchParams({ password: unlockInput.value }),
        });
        if (response.status === 204) {
          const elt = pendingElt;
          unlockDialog.close();
          if (elt && elt.isConnected) {
            retryElt = elt;
            htmx.trigger(
              elt,
              elt instanceof HTMLFormElement ? "submit" : "click",
            );
          }
          return;
        }
        const data = await response.json().catch(() => ({}));
        showUnlockError(data.error || "확인하지 못했습니다. 다시 시도하세요.");
      } catch {
        showUnlockError("확인하지 못했습니다. 다시 시도하세요.");
      } finally {
        submit.disabled = false;
      }
    });
  }
})();
