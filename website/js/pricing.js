(function () {
  const modal = document.getElementById('subscribe-modal');
  const modalTitle = document.getElementById('modal-title');
  const planInput = document.getElementById('plan');
  const form = document.getElementById('subscribe-form');
  const success = document.getElementById('subscribe-success');
  const closeBtn = document.querySelector('.modal-close');
  const cancelBtn = document.getElementById('cancel-subscribe');
  const subscribeButtons = document.querySelectorAll('[data-plan]');

  function openModal(plan) {
    planInput.value = plan;
    modalTitle.textContent = `Subscribe to ${plan}`;
    success.classList.add('hidden');
    form.classList.remove('hidden');
    form.reset();
    planInput.value = plan;
    modal.classList.add('open');
    document.body.style.overflow = 'hidden';
  }

  function closeModal() {
    modal.classList.remove('open');
    document.body.style.overflow = '';
  }

  subscribeButtons.forEach(btn => {
    btn.addEventListener('click', () => {
      const plan = btn.dataset.plan;
      if (plan === 'Free') {
        // Free tier just directs to downloads
        window.location.href = 'downloads.html';
        return;
      }
      openModal(plan);
    });
  });

  closeBtn.addEventListener('click', closeModal);
  cancelBtn.addEventListener('click', closeModal);
  modal.addEventListener('click', (e) => {
    if (e.target === modal) closeModal();
  });
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && modal.classList.contains('open')) closeModal();
  });

  form.addEventListener('submit', (e) => {
    e.preventDefault();
    const email = form.email.value.trim();
    const plan = form.plan.value;
    if (!email || !plan) return;

    // Simulate a backend subscription request.
    // In production this would call your payment/subscription API.
    form.classList.add('hidden');
    success.querySelector('p').textContent = `Thanks! A confirmation link for ${plan} has been sent to ${email}.`;
    success.classList.remove('hidden');
  });
})();
