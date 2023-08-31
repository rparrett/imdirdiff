(function () {
  let xs = document.querySelectorAll(".x");
  xs.forEach((x) =>
    x.addEventListener("click", function (e) {
      {
        e.currentTarget.parentNode.remove();
      }
    })
  );
})();
