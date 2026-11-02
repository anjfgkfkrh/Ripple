// 작은 화면 동작(설계 5.4). 프레임워크 없이 이벤트 위임으로 쓴다.
(() => {
  // 제출 중 단추를 잠그고 글자를 바꾼다. 비밀번호 칸은 잠그지 않는다(잠긴 칸은 제출되지 않는다).
  function lockButton(form) {
    const label = form.dataset.pendingLabel;
    const button = form.querySelector("button[type=submit]");
    if (!label || !button || button.disabled) return;
    button.disabled = true;
    button.textContent = label;
  }

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
})();
