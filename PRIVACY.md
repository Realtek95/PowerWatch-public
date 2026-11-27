# Політика конфіденційності PowerWatch

**Дата оновлення: 27.11.2026**  
**Розробник: bostech**

## 1. Загальні положення

PowerWatch — незалежний застосунок для моніторингу графіків відключення електроенергії. Застосунок не є офіційним продуктом ДТЕК, YASNO або інших операторів систем розподілу електроенергії.

## 2. Дані, які обробляє застосунок

Для забезпечення основних функцій PowerWatch може обробляти:

- Введені користувачем адреси: область, населений пункт, вулицю та номер будинку.
- Назви збережених адрес, операторів та черги відключень.
- Налаштування застосунку та сповіщень.
- Історію графіків відключень і їхніх змін.
- Технічні дані, необхідні для отримання графіків та перевірки оновлень.

PowerWatch не потребує створення облікового запису, не використовує рекламні ідентифікатори, системи аналітики або хмарну синхронізацію.

## 3. Використання даних

Дані використовуються для визначення черги відключень, отримання актуальних графіків, відображення історії, формування локальних сповіщень та забезпечення роботи застосунку.

Розробник не продає персональні дані та не використовує їх для реклами або створення маркетингових профілів.

## 4. Зберігання та захист даних

Адреси, налаштування, кеш та історія зберігаються локально на пристрої користувача в базі даних SQLite.

PowerWatch не має власного віддаленого сервера для збирання або зберігання персональних даних користувачів.

Записи історії графіків, їхніх змін та завершених сповіщень підлягають автоматичному очищенню після 30 днів під час обслуговування локальної бази даних.

Мережеві запити до підтримуваних джерел здійснюються через HTTPS. Захист локальних файлів також залежить від налаштувань безпеки операційної системи користувача.

## 5. Сторонні сервіси

Для отримання актуальної інформації PowerWatch може звертатися до таких зовнішніх джерел:

- ДТЕК — https://www.dtek-dnem.com.ua/
- YASNO — https://yasno.com.ua/
- SvitloPulse — https://svitlopulse.andrewboyko.name/
- GitHub — https://github.com/

Для визначення черги або адресного статусу відповідному джерелу можуть передаватися населений пункт, вулиця, номер будинку або відповідні ідентифікатори адреси. Для отримання групових графіків можуть передаватися дані про оператора та чергу відключень.

Зовнішні сервіси можуть отримувати IP-адресу пристрою та стандартні технічні відомості мережевого запиту.

GitHub може використовуватися як джерело резервних даних та для перевірки оновлень у відповідних версіях застосунку.

Обробка інформації сторонніми сервісами регулюється їхніми власними політиками конфіденційності.

## 6. Керування даними

Користувач може переглядати та змінювати збережені адреси, видаляти їх, налаштовувати автоматичне оновлення графіків і керувати сповіщеннями.

Для припинення автоматичних мережевих запитів користувач може припинити роботу застосунку та вимкнути його автоматичний запуск.

Для повного видалення локальних даних можна видалити дані застосунку засобами операційної системи або видалити відповідні локальні файли.

Користувач також може звернутися до розробника з питань доступу, виправлення або видалення персональних даних.

## 7. Конфіденційність дітей

PowerWatch не призначений спеціально для дітей та не здійснює цілеспрямованого збирання їхніх персональних даних.

## 8. Зміни політики конфіденційності

Політика конфіденційності може оновлюватися у зв’язку зі змінами функціональності застосунку або законодавчих вимог. Актуальна версія публікується в офіційному репозиторії проєкту.

## 9. Контактна інформація

Розробник: **bostech**

З питань конфіденційності та обробки даних можна звернутися через офіційний репозиторій:

https://github.com/Realtek95/PowerWatch-public

Не публікуйте особисті адреси або інші конфіденційні дані у відкритих повідомленнях про помилки.

---

# PowerWatch Privacy Policy

**Last updated: November 27, 2026**  
**Developer: bostech**

## 1. General information

PowerWatch is an independent application designed to monitor electricity outage schedules. It is not an official product of DTEK, YASNO, or other electricity distribution operators.

## 2. Information processed

To provide its core functionality, PowerWatch may process:

- User-entered addresses, including region, locality, street and building number.
- Saved address labels, electricity operators and outage groups.
- Application and notification settings.
- Outage schedules and change history.
- Technical information required to retrieve schedules and check for updates.

PowerWatch does not require user accounts or use advertising identifiers, analytics systems or cloud synchronization.

## 3. Use of information

Information is used to identify outage groups, retrieve current schedules, display history, generate local notifications and operate the application.

The developer does not sell personal information or use it for advertising or marketing profiles.

## 4. Data storage and security

Addresses, settings, cached data and history are stored locally on the user's device in an SQLite database.

PowerWatch does not operate its own remote server for collecting or storing users' personal information.

Historical schedule records, schedule changes and completed notification records are subject to automatic cleanup after 30 days during local database maintenance.

Network requests to supported sources use HTTPS. Local file protection also depends on the security settings of the user's operating system.

## 5. Third-party services

PowerWatch may communicate with the following external services:

- DTEK — https://www.dtek-dnem.com.ua/
- YASNO — https://yasno.com.ua/
- SvitloPulse — https://svitlopulse.andrewboyko.name/
- GitHub — https://github.com/

Locality, street, building number or corresponding address identifiers may be transmitted to the relevant provider to determine an outage group or address-specific status. Operator and outage group information may be transmitted to retrieve group schedules.

External services may receive the device's IP address and standard technical information associated with network requests.

GitHub may be used as a fallback data source and to check for updates in applicable application versions.

Information processed by third-party services is subject to their respective privacy policies.

## 6. User controls

Users can view, modify and delete saved addresses, configure automatic schedule updates and manage notifications.

Users can stop automatic network requests by closing the application and disabling its automatic startup.

To remove all locally stored information, users can clear application data using operating system tools or delete the corresponding local files.

Users may also contact the developer regarding access to, correction or deletion of personal information.

## 7. Children's privacy

PowerWatch is not specifically directed at children and does not intentionally collect their personal information.

## 8. Changes to this policy

This Privacy Policy may be updated to reflect changes in application functionality or legal requirements. The current version is published in the official project repository.

## 9. Contact information

Developer: **bostech**

For privacy-related inquiries, please contact the developer through the official project repository:

https://github.com/Realtek95/PowerWatch-public

Do not include personal addresses or other confidential information in public bug reports.
